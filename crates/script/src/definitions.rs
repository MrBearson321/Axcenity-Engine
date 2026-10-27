//! Writes the luau-lsp definitions file: `declare extern type` and `declare`
//! statements describing the whole script API, with `---` doc comments, so
//! editors such as VS Code can autocomplete it and type-check scripts as Luau.
//!
//! - Classes, properties, events and enums come straight from `core`, so they
//!   can't fall out of date. A new property only needs a line in
//!   `PROPERTY_DOCS`, and a new event one in `EVENT_DOCS`.
//! - APIs that only work on one side (see `sides`) say so in their
//!   description, added from `SIDE_ONLY_APIS`.
//! - Functions, globals and value types are described by hand below.
//! - Luau's own globals (`math`, `bit32`, `buffer`, `typeof`, ...) are built
//!   into luau-lsp, so they aren't declared here.
//! - Tests check the descriptions against what scripts really get, and that
//!   the checked-in file matches this generator.
//! - [`api_reference`] lists the same declarations, with the same
//!   descriptions, for the editor's searchable API reference.
//!
//! luau-lsp calls the format unstable and undocumented; it is Luau's own
//! definition syntax, as in luau-lsp's Roblox definitions.

use axcenity_core::{ClassName, ValueKind, World};

use crate::sides::{Side, side_only};

mod docs;
mod tables;

use crate::types::EnumType;
use docs::*;
use tables::*;

/// Where the generated file is kept, relative to the repository root.
pub const DEFINITIONS_PATH: &str = "definitions/axcenity.d.luau";

/// The command that rewrites the file, shown in its header and in test failures.
const REGENERATE: &str = "cargo run -p axcenity-script --bin lua-definitions";

/// Children found by name, such as `workspace.Pad`. A child could be of any
/// class, and Luau only lets you use a property every member of a union has,
/// so a union of every class would flag `workspace.Pad.Color = ...`; `any`
/// lets it through unchecked. Every Instance class needs its own: Luau
/// doesn't inherit indexers.
const CHILD_INDEXER: &str = "\t[string]: any";

/// The class of `Enum.<name>`'s items, such as `EnumMaterial`. Luau class
/// names can't have dots, so this is the type for `Enum.Material.Neon`.
fn enum_item_class(enum_type: EnumType) -> String {
    format!("Enum{}", enum_type.name())
}

/// The Luau type for a property of this kind.
fn luau_type(kind: ValueKind) -> String {
    match kind {
        ValueKind::Bool => "boolean".to_owned(),
        ValueKind::Number => "number".to_owned(),
        ValueKind::String => "string".to_owned(),
        ValueKind::Vector3 => "Vector3".to_owned(),
        ValueKind::Vector2 => "Vector2".to_owned(),
        ValueKind::UDim2 => "UDim2".to_owned(),
        ValueKind::Color3 => "Color3".to_owned(),
        ValueKind::CFrame => "CFrame".to_owned(),
        // No property holds one yet; attributes do (`GetAttribute` is `any`).
        ValueKind::EnumItem => "any".to_owned(),
        ValueKind::Instance => "Instance?".to_owned(),
        ValueKind::PhysicalProperties => "PhysicalProperties?".to_owned(),
        // Enum properties read as items and also accept item names.
        ValueKind::Material
        | ValueKind::PartType
        | ValueKind::TextureChannel
        | ValueKind::ParticleType
        | ValueKind::ToneMapping => {
            let enum_type = EnumType::of_kind(kind).expect("an enum kind");
            let mut options = vec![enum_item_class(enum_type)];
            options.extend(
                enum_type
                    .item_names()
                    .iter()
                    .map(|name| format!("\"{name}\"")),
            );
            options.join(" | ")
        }
    }
}

/// `name: type` for each parameter. `("x?", "number")` becomes `x: number?`,
/// and `("...", "any")` becomes `...: any` in a declared function, or
/// `...any` in a function type (`in_type`), as Luau writes them.
fn parameters(function: &Function, in_type: bool) -> Vec<String> {
    function
        .params
        .iter()
        .map(|&(name, ty)| match name.strip_suffix('?') {
            // A union or function type needs brackets before `?`.
            Some(name) if ty.contains(['|', '-']) => format!("{name}: ({ty})?"),
            Some(name) => format!("{name}: {ty}?"),
            None if name == "..." && in_type => format!("...{ty}"),
            None => format!("{name}: {ty}"),
        })
        .collect()
}

/// What an [`ApiEntry`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiKind {
    /// An Instance class, such as `Part`.
    Class,
    /// A value that isn't an Instance, such as `Vector3` or `Signal`.
    Type,
    /// An enum, such as `Enum.Material`.
    Enum,
    /// A global table of functions with no type of the same name, such as `task`.
    Library,
    /// A global variable, such as `workspace`.
    Global,
    Property,
    Event,
    /// A function or method.
    Function,
    /// One choice of an enum, such as `Enum.Material.Neon`.
    EnumItem,
}

impl ApiKind {
    /// What the reference calls it, such as "Property".
    pub fn label(self) -> &'static str {
        match self {
            ApiKind::Class => "Class",
            ApiKind::Type => "Type",
            ApiKind::Enum => "Enum",
            ApiKind::Library => "Library",
            ApiKind::Global => "Global",
            ApiKind::Property => "Property",
            ApiKind::Event => "Event",
            ApiKind::Function => "Function",
            ApiKind::EnumItem => "Enum item",
        }
    }
}

/// One declaration in the definitions file, for the editor's API reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiEntry {
    pub kind: ApiKind,
    pub name: String,
    /// The class, type, enum or library it's a member of, such as `Part`.
    /// Empty for one of those itself, and for globals.
    pub owner: String,
    /// How it's written or called, such as `Part.Size: Vector3` or
    /// `Instance:FindFirstChild(name: string, recursive: boolean?): Instance?`.
    pub signature: String,
    /// What it does, in the definitions' words. Empty if they have none.
    pub doc: String,
}

impl ApiEntry {
    /// `Owner.Name`, or the name alone without an owner.
    pub fn qualified_name(&self) -> String {
        if self.owner.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.owner, self.name)
        }
    }
}

/// Collects the file's text one line at a time, and the reference entry for
/// each declaration as it's written.
#[derive(Default)]
struct Writer {
    text: String,
    /// What the members being written belong to, such as `Part`.
    owner: String,
    reference: Vec<ApiEntry>,
}

impl Writer {
    fn line(&mut self, text: &str) {
        self.text.push_str(text);
        self.text.push('\n');
    }

    /// Adds a member of the current owner to the reference.
    fn entry(&mut self, kind: ApiKind, name: &str, signature: String, doc: &str) {
        self.reference.push(ApiEntry {
            kind,
            name: name.to_owned(),
            owner: self.owner.clone(),
            signature,
            doc: doc.to_owned(),
        });
    }

    /// Adds a class, type, enum, library or global to the reference. The
    /// members written after it belong to it.
    fn top(&mut self, kind: ApiKind, name: &str, signature: String, doc: &str) {
        self.owner.clear();
        self.entry(kind, name, signature, doc);
        self.owner = name.to_owned();
    }

    /// A doc comment, which luau-lsp shows on hover and in autocomplete.
    fn doc(&mut self, indent: &str, text: &str) {
        if !text.is_empty() {
            self.line(&format!("{indent}--- {text}"));
        }
    }

    /// A class member: `Name: type`, which the reference shows as
    /// `Owner.Name: type`.
    fn member(&mut self, kind: ApiKind, name: &str, ty: &str, doc: &str) {
        self.doc("\t", doc);
        self.line(&format!("\t{name}: {ty}"));
        let signature = format!("{}.{name}: {ty}", self.owner);
        self.entry(kind, name, signature, doc);
    }

    fn field(&mut self, field: &Field) {
        let doc = with_side(field.name, field.doc);
        self.member(ApiKind::Property, field.name, field.ty, &doc);
    }

    /// A method, called with `:`.
    fn method(&mut self, method: &Function) {
        let doc = with_side(method.name, method.doc);
        self.doc("\t", &doc);
        let params = parameters(method, false).join(", ");
        let with_self = if params.is_empty() {
            "self".to_owned()
        } else {
            format!("self, {params}")
        };
        self.line(&format!(
            "\tfunction {}({with_self}): {}",
            method.name, method.returns
        ));
        let signature = format!(
            "{}:{}({params}): {}",
            self.owner, method.name, method.returns
        );
        self.entry(ApiKind::Function, method.name, signature, &doc);
    }

    fn heading(&mut self, title: &str) {
        // Sections already end with a blank line.
        if !self.text.ends_with("\n\n") {
            self.line("");
        }
        self.line(&format!("-- {title} {}", "-".repeat(74 - title.len())));
        self.line("");
    }

    /// Ends a `declare extern type` and leaves a blank line after it.
    fn end(&mut self) {
        self.line("end");
        self.line("");
    }
}

/// The text of the definitions file for the current API.
pub fn lua_definitions() -> String {
    write().text
}

/// Every class, type, enum, global and member the definitions file declares,
/// in its order and with its descriptions. It comes from the same writer, so
/// it changes whenever the file does.
pub fn api_reference() -> Vec<ApiEntry> {
    write().reference
}

/// Writes the definitions file and its reference entries.
fn write() -> Writer {
    let mut w = Writer::default();
    w.line("-- The Axcenity Engine script API, for luau-lsp (autocomplete and type");
    w.line("-- checking in VS Code and other editors).");
    w.line(&format!(
        "-- Generated by `{REGENERATE}`. Don't edit by hand."
    ));

    w.heading("Values");
    for value_type in VALUE_TYPES {
        let class = value_type.class;
        w.doc("", value_type.doc);
        w.line(&format!("declare extern type {class} with"));
        w.top(ApiKind::Type, class, class.to_owned(), value_type.doc);
        for field in value_type.fields {
            w.field(field);
        }
        for operator in value_type.operators {
            w.line(&format!("\tfunction {operator}"));
        }
        for method in value_type.methods {
            w.method(method);
        }
        // A data row's columns are the project's, so any name reads.
        if class == "DataRow" {
            w.line(CHILD_INDEXER);
        }
        w.end();
    }

    w.heading("Enums");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        let item = enum_item_class(enum_type);
        let item_doc = format!("One choice of `Enum.{name}`.");
        w.doc("", &item_doc);
        w.line(&format!("declare extern type {item} extends EnumItem with"));
        w.end();
        let doc = format!("The choices of `Enum.{name}`.");
        w.doc("", &doc);
        w.line(&format!("declare extern type {item}Items with"));
        let qualified = format!("Enum.{name}");
        w.top(ApiKind::Enum, &qualified, qualified.clone(), &doc);
        for choice in enum_type.item_names() {
            w.line(&format!("\t{choice}: {item}"));
            let signature = format!("{qualified}.{choice}");
            w.entry(ApiKind::EnumItem, choice, signature, &item_doc);
        }
        w.end();
    }
    w.doc("", ENUM_GLOBAL_DOC);
    w.line("declare extern type EnumLibrary with");
    for enum_type in EnumType::ALL {
        let name = enum_type.name();
        w.line(&format!("\t{name}: {}Items", enum_item_class(enum_type)));
    }
    w.end();

    w.heading("Instances");
    w.doc("", SIGNAL_DOC);
    w.line("declare extern type Signal with");
    w.top(ApiKind::Type, "Signal", "Signal".to_owned(), SIGNAL_DOC);
    for method in SIGNAL_METHODS {
        w.method(method);
    }
    w.end();
    w.doc("", CONNECTION_DOC);
    w.line("declare extern type Connection with");
    w.top(
        ApiKind::Type,
        "Connection",
        "Connection".to_owned(),
        CONNECTION_DOC,
    );
    for field in CONNECTION_FIELDS {
        w.field(field);
    }
    for method in CONNECTION_METHODS {
        w.method(method);
    }
    w.end();

    let instance_doc = "Anything in the game tree. Every class below is a kind of Instance.";
    w.doc("", instance_doc);
    w.line("declare extern type Instance with");
    w.top(
        ApiKind::Class,
        "Instance",
        "Instance".to_owned(),
        instance_doc,
    );
    for field in INSTANCE_FIELDS {
        w.field(field);
    }
    for (event, doc) in INSTANCE_EVENT_DOCS {
        w.member(ApiKind::Event, event, "Signal", doc);
    }
    w.line(CHILD_INDEXER);
    for method in INSTANCE_METHODS {
        w.method(method);
    }
    w.end();

    // The services, as `game` holds them, so `game.Workspace` autocompletes.
    let world = World::new();
    let services: Vec<ClassName> = world
        .get(world.root())
        .map(|game| {
            game.children()
                .iter()
                .filter_map(|&id| world.get(id).map(|s| s.class()))
                .collect()
        })
        .unwrap_or_default();

    for class in ClassName::ALL {
        w.doc("", class_doc(class));
        w.line(&format!(
            "declare extern type {class} extends Instance with"
        ));
        w.top(
            ApiKind::Class,
            class.as_str(),
            class_signature(class),
            class_doc(class),
        );
        if class == ClassName::DataModel {
            for &service in &services {
                let name = service.as_str();
                w.member(ApiKind::Property, name, name, class_doc(service));
            }
        }
        for (name, default) in class.default_properties() {
            let doc = property_doc(class, name).unwrap_or("");
            w.member(ApiKind::Property, name, &luau_type(default.kind()), doc);
        }
        for (_, field) in CLASS_FIELDS.iter().filter(|(c, _)| *c == class) {
            w.field(field);
        }
        for &event in class.events() {
            let doc = event_doc(class, event).unwrap_or("");
            w.member(ApiKind::Event, event, "Signal", &with_side(event, doc));
        }
        w.line(CHILD_INDEXER);
        for (_, method) in CLASS_METHODS.iter().filter(|(c, _)| *c == class) {
            w.method(method);
        }
        w.end();
    }

    w.heading("Globals");
    for &(name, ty, doc) in INSTANCE_GLOBALS {
        w.doc("", doc);
        w.line(&format!("declare {name}: {ty}"));
        w.line("");
        w.top(ApiKind::Global, name, format!("{name}: {ty}"), doc);
    }
    w.doc("", ENUM_GLOBAL_DOC);
    w.line("declare Enum: EnumLibrary");
    w.line("");
    w.top(ApiKind::Global, "Enum", "Enum".to_owned(), ENUM_GLOBAL_DOC);
    w.owner.clear();
    for function in GLOBAL_FUNCTIONS {
        let doc = with_side(function.name, function.doc);
        let signature = format!(
            "{}({}): {}",
            function.name,
            parameters(function, false).join(", "),
            function.returns
        );
        w.doc("", &doc);
        w.line(&format!("declare function {signature}"));
        w.line("");
        w.entry(ApiKind::Function, function.name, signature, &doc);
    }

    // Each library is a class of function fields (called with `.`), so its
    // functions can have doc comments, and one global holds it.
    for library in LIBRARIES {
        let class = format!("{}Library", library.global);
        w.doc("", library.doc);
        w.line(&format!("declare extern type {class} with"));
        // `Vector3.new` belongs with the Vector3 type; `task` has no type.
        let global = library.global;
        let has_type = w
            .reference
            .iter()
            .any(|entry| entry.owner.is_empty() && entry.name == global);
        if has_type {
            w.owner = global.to_owned();
        } else {
            w.top(ApiKind::Library, global, global.to_owned(), library.doc);
        }
        for constant in library.constants {
            w.doc("\t", constant.doc);
            w.line(&format!("\t{}: {}", constant.name, constant.ty));
            let signature = format!("{global}.{}: {}", constant.name, constant.ty);
            w.entry(ApiKind::Property, constant.name, signature, constant.doc);
        }
        for function in library.functions {
            let doc = with_side(function.name, function.doc);
            w.doc("\t", &doc);
            let ty = function_type(library, function);
            w.line(&format!("\t{}: {ty}", function.name));
            let signature = format!(
                "{global}.{}({}): {}",
                function.name,
                parameters(function, false).join(", "),
                function.returns
            );
            w.entry(ApiKind::Function, function.name, signature, &doc);
        }
        w.end();
        w.doc("", library.doc);
        w.line(&format!("declare {}: {class}", library.global));
        w.line("");
    }

    // One blank line at the end, not two.
    while w.text.ends_with("\n\n") {
        w.text.pop();
    }
    w
}

/// How scripts reach a class: `game.Workspace` for a service, or
/// `Instance.new("Part")` for one they can make.
fn class_signature(class: ClassName) -> String {
    match class {
        ClassName::DataModel => "game".to_owned(),
        _ if class.is_service() => format!("game.{class}"),
        ClassName::Player => class.to_string(),
        _ => format!("Instance.new(\"{class}\")"),
    }
}

/// A library function's type, such as `(x: number?) -> Vector3`.
/// `Instance.new("Part")` returns a `Part`, so `Instance.new` is the
/// intersection of one function per class (Luau's way to write overloads),
/// then the plain one for any other name.
fn function_type(library: &Library, function: &Function) -> String {
    let plain = format!(
        "({}) -> {}",
        parameters(function, true).join(", "),
        function.returns
    );
    if library.global != "Instance" || function.name != "new" {
        return plain;
    }
    let mut overloads: Vec<String> = ClassName::ALL
        .into_iter()
        // Services always exist, and Players are made when someone joins.
        .filter(|class| !class.is_service() && *class != ClassName::Player)
        .map(|class| format!("((className: \"{class}\") -> {class})"))
        .collect();
    overloads.push(format!("({plain})"));
    overloads.join(" & ")
}

/// `doc`, saying where it works if `name` is a side-only API.
fn with_side(name: &str, doc: &str) -> String {
    match side_only(name).map(|api| api.side) {
        Some(Side::Client) => format!("{doc} LocalScripts only."),
        Some(Side::Server) => format!("{doc} Scripts only."),
        None => doc.to_owned(),
    }
}

fn event_doc(class: ClassName, event: &str) -> Option<&'static str> {
    EVENT_DOCS
        .iter()
        .find(|(c, e, _)| *c == class && *e == event)
        .map(|(_, _, doc)| *doc)
}

fn property_doc(class: ClassName, property: &str) -> Option<&'static str> {
    PROPERTY_DOCS
        .iter()
        .find(|(c, p, _)| *c == class && *p == property)
        .map(|(_, _, doc)| *doc)
}

#[cfg(test)]
mod tests;
