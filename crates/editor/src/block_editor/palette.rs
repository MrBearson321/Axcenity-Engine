use super::*;

/// A block or reporter in the palette.
#[derive(Clone, Debug)]
pub(super) enum PaletteItem {
    /// Boxed: a block is far bigger than a reporter.
    Block(Box<BlockKind>),
    Reporter(Expression),
    /// The "Make a Variable" button.
    NewVariable,
}

/// The palette's blocks for `category`. Object menus start on the first
/// object of their kind, so a new block usually works as it is.
pub(super) fn palette(
    category: Category,
    client: bool,
    objects: &[ObjectInfo],
    variables: &[String],
) -> Vec<PaletteItem> {
    let (mut items, luau) = palette_parts(category, client, objects, variables);
    items.extend(luau);
    items
}

/// [`palette`]'s blocks in two lists: the category's own, then the blocks
/// for Luau itself (`local`, `while`, a `:Connect` …), which the Blocks
/// palette folds away under "Lua blocks" until opened.
pub(super) fn palette_parts(
    category: Category,
    client: bool,
    objects: &[ObjectInfo],
    variables: &[String],
) -> (Vec<PaletteItem>, Vec<PaletteItem>) {
    let first = |filter: Filter| {
        objects
            .iter()
            .find(|object| filter.matches(object.class))
            .map_or_else(
                || filter.placeholder().to_owned(),
                |object| object.path.clone(),
            )
    };
    let number = |value| Expression::Number { value };
    let name = |name: &str| Expression::Name {
        name: name.to_owned(),
    };
    let text = |value: &str| Expression::Text {
        value: value.to_owned(),
    };
    let vector = |x, y, z| Expression::Vector3 { x, y, z };
    let condition = || Expression::Boolean { value: false };
    let part = first(Filter::Parts);
    let sound = first(Filter::Sounds);
    // The first BindableEvent in ReplicatedStorage, for the event blocks.
    let bindable = objects
        .iter()
        .find(|object| object.path == STORAGE_PATH && object.class == ClassName::ReplicatedStorage)
        .and_then(|storage| {
            storage
                .names
                .iter()
                .find(|(kind, _)| *kind == NameKind::Bindable)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| NameKind::Bindable.example().to_owned());
    // The first data table (or its first column) the project has.
    let storage_name = |kind: NameKind| {
        objects
            .iter()
            .find(|object| object.path == STORAGE_PATH)
            .and_then(|storage| storage.names.iter().find(|(of, _)| *of == kind))
            .map_or_else(|| kind.example().to_owned(), |(_, name)| name.clone())
    };
    let variable = variables
        .first()
        .cloned()
        .unwrap_or_else(|| "score".to_owned());
    use PaletteItem::Reporter as R;
    let block = |kind| PaletteItem::Block(Box::new(kind));
    let mut items = match category {
        Category::Events => vec![
            block(BlockKind::WhenStarted { body: Vec::new() }),
            block(BlockKind::WhenTouched {
                part: part.clone(),
                body: Vec::new(),
            }),
            block(BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::WhenPromptTriggered {
                prompt: first(Filter::Prompts),
                body: Vec::new(),
            }),
            block(BlockKind::WhenPartClicked {
                detector: first(Filter::ClickDetectors),
                body: Vec::new(),
            }),
            block(BlockKind::WhenCommandRun {
                command: text("heal"),
                help: text("Heals the player"),
                body: Vec::new(),
            }),
            block(BlockKind::WhenClicked {
                button: first(Filter::Buttons),
                body: Vec::new(),
            }),
            block(BlockKind::WhenMouseEnters {
                element: first(Filter::Gui),
                body: Vec::new(),
            }),
            block(BlockKind::WhenMouseLeaves {
                element: first(Filter::Gui),
                body: Vec::new(),
            }),
            block(BlockKind::WhenValueChanged {
                widget: first(Filter::Widgets),
                body: Vec::new(),
            }),
            block(BlockKind::EveryFrame { body: Vec::new() }),
            block(BlockKind::BeforePhysics { body: Vec::new() }),
            block(BlockKind::BeforeRender { body: Vec::new() }),
            block(BlockKind::WhenEventFired {
                event: bindable.clone(),
                body: Vec::new(),
            }),
            block(BlockKind::FireEvent {
                event: bindable.clone(),
            }),
            block(BlockKind::WhenSoundEnded {
                sound: sound.clone(),
                body: Vec::new(),
            }),
            block(BlockKind::WhenPropertyChanged {
                target: part.clone(),
                property: "Transparency".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::WhenAttributeChanged {
                target: part.clone(),
                attribute: NameKind::Attribute.example().to_owned(),
                body: Vec::new(),
            }),
        ],
        Category::Control => vec![
            block(BlockKind::Wait {
                seconds: number(1.0),
            }),
            block(BlockKind::Repeat {
                count: number(10.0),
                body: Vec::new(),
            }),
            block(BlockKind::Forever { body: Vec::new() }),
            block(BlockKind::If {
                condition: condition(),
                body: Vec::new(),
            }),
            block(BlockKind::IfElse {
                condition: condition(),
                then_body: Vec::new(),
                else_body: Vec::new(),
            }),
            block(BlockKind::WaitUntil {
                condition: condition(),
            }),
            block(BlockKind::RepeatUntil {
                condition: condition(),
                body: Vec::new(),
            }),
            block(BlockKind::StopScript),
            block(BlockKind::ForEachChild {
                target: String::new(),
                variable: "child".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::ForEachPartInRadius {
                center: vector(0.0, 0.0, 0.0),
                radius: number(10.0),
                variable: "part".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::ForEachPartTouching {
                target: part.clone(),
                variable: "part".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::WaitForChild {
                target: String::new(),
                name: NameKind::Child.example().to_owned(),
            }),
            block(BlockKind::WaitForEvent {
                target: part.clone(),
                event: NameKind::Signal.example().to_owned(),
            }),
            block(BlockKind::DestroyAfter {
                target: part.clone(),
                seconds: number(2.0),
            }),
        ],
        Category::Motion => vec![
            block(BlockKind::MoveTo {
                target: part.clone(),
                position: vector(0.0, 5.0, 0.0),
            }),
            block(BlockKind::MoveBy {
                target: part.clone(),
                offset: vector(0.0, 1.0, 0.0),
            }),
            block(BlockKind::MoveForward {
                target: part.clone(),
                distance: number(5.0),
            }),
            block(BlockKind::LookAt {
                target: part.clone(),
                position: vector(0.0, 0.0, 0.0),
            }),
            block(BlockKind::TurnTo {
                target: part.clone(),
                orientation: vector(0.0, 0.0, 0.0),
            }),
            block(BlockKind::TurnBy {
                target: part.clone(),
                rotation: vector(0.0, 15.0, 0.0),
            }),
            block(BlockKind::SetAnchored {
                target: part.clone(),
                anchored: Expression::Boolean { value: true },
            }),
            block(BlockKind::PushWithForce {
                target: part.clone(),
                force: vector(0.0, 50.0, 0.0),
            }),
            block(BlockKind::SetPartVelocity {
                target: part.clone(),
                velocity: vector(0.0, 20.0, 0.0),
            }),
            block(BlockKind::SetFriction {
                target: part.clone(),
                friction: number(0.1),
            }),
            block(BlockKind::SetDestination {
                agent: first(Filter::Agents),
                position: vector(10.0, 0.0, 0.0),
            }),
            block(BlockKind::SetVelocity {
                agent: first(Filter::Agents),
                velocity: vector(0.0, 0.0, 0.0),
            }),
            block(BlockKind::WhenAgentSenses {
                agent: first(Filter::Agents),
                sense: axcenity_blocks::SENSES[0].to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::Patrol {
                agent: first(Filter::Agents),
                spline: first(Filter::Splines),
            }),
            block(BlockKind::PlaceOnSpline {
                target: part.clone(),
                spline: first(Filter::Splines),
                distance: number(0.0),
            }),
            block(BlockKind::WhenSplineReached {
                follower: first(Filter::SplineFollowers),
                which: axcenity_blocks::SPLINE_ENDS[0].to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::WhenSeatOccupied {
                seat: first(Filter::Seats),
                body: Vec::new(),
            }),
            block(BlockKind::LeaveSeat {
                controller: first(Filter::Controllers),
            }),
            block(BlockKind::SetCameraType {
                camera_type: axcenity_blocks::CAMERA_TYPES[0].to_owned(),
            }),
            block(BlockKind::PointCameraAt {
                position: vector(0.0, 0.0, 0.0),
            }),
            block(BlockKind::ShakeCamera {
                intensity: number(1.0),
                seconds: number(0.5),
            }),
        ],
        Category::Looks => vec![
            block(BlockKind::SpawnDecal {
                texture: Expression::Text {
                    value: "assets/textures/hole.png".to_owned(),
                },
                position: vector(0.0, 0.0, 0.0),
                facing: vector(0.0, 1.0, 0.0),
                size: number(1.0),
                seconds: number(10.0),
            }),
            block(BlockKind::SetColor {
                target: part.clone(),
                color: Expression::Color3 {
                    r: 1.0,
                    g: 0.3,
                    b: 0.3,
                },
            }),
            block(BlockKind::SetText {
                target: first(Filter::Texts),
                text: text("Hello!"),
            }),
            block(BlockKind::Show {
                target: first(Filter::Gui),
            }),
            block(BlockKind::Hide {
                target: first(Filter::Gui),
            }),
            block(BlockKind::ShowPage {
                layout: first(Filter::PageLayouts),
                page: text("Home"),
            }),
            block(BlockKind::ScrollTo {
                target: first(Filter::ScrollingFrames),
                x: number(0.0),
                y: number(0.0),
            }),
            block(BlockKind::SetValue {
                target: first(Filter::Widgets),
                value: number(50.0),
            }),
            block(BlockKind::SetProperty {
                target: part.clone(),
                property: "Size".to_owned(),
                value: vector(4.0, 1.0, 2.0),
            }),
            block(BlockKind::SetAttribute {
                target: part.clone(),
                attribute: NameKind::Attribute.example().to_owned(),
                value: number(100.0),
            }),
            block(BlockKind::SetLanguage {
                language: text("English"),
            }),
            block(BlockKind::SetToneMapping {
                curve: "Neutral".to_owned(),
            }),
            block(BlockKind::SetClockTime { time: number(18.0) }),
            block(BlockKind::SetAtmosphere {
                property: "Haze".to_owned(),
                value: number(2.0),
            }),
            block(BlockKind::SetGraphicsQuality {
                quality: "High".to_owned(),
            }),
            block(BlockKind::SetGraphicsOption {
                setting: "ShadowQuality".to_owned(),
                choice: "High".to_owned(),
            }),
            block(BlockKind::SetGraphicsNumber {
                setting: "RenderScale".to_owned(),
                value: number(1.0),
            }),
        ],
        Category::Sound => vec![
            block(BlockKind::PlaySound {
                target: sound.clone(),
            }),
            block(BlockKind::StopSound {
                target: sound.clone(),
            }),
            block(BlockKind::PauseSound {
                target: sound.clone(),
            }),
            block(BlockKind::ResumeSound {
                target: sound.clone(),
            }),
            block(BlockKind::SetVolume {
                target: sound,
                volume: number(0.5),
            }),
        ],
        Category::Operators => {
            let math = |op| {
                R(Expression::Math {
                    op,
                    left: Box::new(number(0.0)),
                    right: Box::new(number(0.0)),
                })
            };
            let compare = |op, right| {
                R(Expression::Compare {
                    op,
                    left: Box::new(text("")),
                    right: Box::new(number(right)),
                })
            };
            let logic = |op| {
                R(Expression::Logic {
                    op,
                    left: Box::new(condition()),
                    right: Box::new(condition()),
                })
            };
            vec![
                math(MathOp::Add),
                math(MathOp::Subtract),
                math(MathOp::Multiply),
                math(MathOp::Divide),
                R(Expression::Random {
                    min: Box::new(number(1.0)),
                    max: Box::new(number(10.0)),
                }),
                R(Expression::RandomNumber {
                    min: Box::new(number(0.0)),
                    max: Box::new(number(1.0)),
                }),
                R(Expression::Lerp {
                    from: Box::new(vector(0.0, 0.0, 0.0)),
                    to: Box::new(vector(0.0, 10.0, 0.0)),
                    alpha: Box::new(number(0.5)),
                }),
                compare(Comparison::Greater, 50.0),
                compare(Comparison::Less, 50.0),
                compare(Comparison::Equal, 50.0),
                logic(LogicOp::And),
                logic(LogicOp::Or),
                R(Expression::Not {
                    value: Box::new(condition()),
                }),
                R(Expression::Join {
                    left: Box::new(text("apple ")),
                    right: Box::new(text("banana")),
                }),
                math(MathOp::Modulo),
                R(Expression::Function {
                    function: MathFunction::Round,
                    value: Box::new(number(0.0)),
                }),
            ]
        }
        Category::Variables => {
            let mut items = vec![PaletteItem::NewVariable];
            items.extend(
                variables
                    .iter()
                    .map(|name| R(Expression::Variable { name: name.clone() })),
            );
            items.push(block(BlockKind::SetVariable {
                name: variable.clone(),
                value: number(0.0),
            }));
            items.push(block(BlockKind::ChangeVariable {
                name: variable,
                by: number(1.0),
            }));
            items
        }
        Category::Sensing => vec![
            R(Expression::TouchingPart),
            R(Expression::PartUnderMouse),
            R(Expression::KeyDown {
                key: "Space".to_owned(),
            }),
            R(Expression::Property {
                target: part.clone(),
                property: "Position".to_owned(),
            }),
            R(Expression::Attribute {
                target: part.clone(),
                attribute: NameKind::Attribute.example().to_owned(),
            }),
            R(Expression::Raycast {
                origin: Box::new(vector(0.0, 10.0, 0.0)),
                direction: Box::new(vector(0.0, -20.0, 0.0)),
            }),
            R(Expression::RaycastIgnoring {
                origin: Box::new(vector(0.0, 10.0, 0.0)),
                direction: Box::new(vector(0.0, -20.0, 0.0)),
                ignore: part,
            }),
            R(Expression::SplinePosition {
                spline: first(Filter::Splines),
                t: Box::new(number(0.5)),
            }),
            R(Expression::SplineClosestDistance {
                spline: first(Filter::Splines),
                position: Box::new(vector(0.0, 0.0, 0.0)),
            }),
            R(Expression::IsA {
                value: Box::new(Expression::TouchingPart),
                class: "BasePart".to_owned(),
            }),
        ],
        Category::Animation => {
            let animator = first(Filter::Animators);
            let name = |kind: NameKind| {
                objects
                    .iter()
                    .find(|object| object.path == animator)
                    .and_then(|object| object.names.iter().find(|(of, _)| *of == kind))
                    .map_or_else(|| kind.example().to_owned(), |(_, name)| name.clone())
            };
            vec![
                block(BlockKind::SetAnimatorTrigger {
                    animator: animator.clone(),
                    parameter: name(NameKind::Trigger),
                }),
                block(BlockKind::SetAnimatorBool {
                    animator: animator.clone(),
                    parameter: name(NameKind::Bool),
                    value: Expression::Boolean { value: true },
                }),
                block(BlockKind::SetAnimatorNumber {
                    animator: animator.clone(),
                    parameter: name(NameKind::Number),
                    value: number(1.0),
                }),
                block(BlockKind::ResetAnimatorTrigger {
                    animator: animator.clone(),
                    parameter: name(NameKind::Trigger),
                }),
                block(BlockKind::SetAnimatorLayerWeight {
                    animator: animator.clone(),
                    layer: name(NameKind::Layer),
                    weight: number(1.0),
                    seconds: number(0.25),
                }),
                block(BlockKind::PlayAnimatorState {
                    animator: animator.clone(),
                    state: name(NameKind::State),
                }),
                block(BlockKind::PlayAnimation {
                    animator: animator.clone(),
                    animation: first(Filter::Animations),
                }),
                block(BlockKind::PlayCutscene {
                    sequence: first(Filter::Sequences),
                }),
                block(BlockKind::WhenCutsceneFinished {
                    sequence: first(Filter::Sequences),
                    body: Vec::new(),
                }),
                block(BlockKind::WhenStateChanged {
                    animator: animator.clone(),
                    body: Vec::new(),
                }),
                block(BlockKind::WhenAnimationEvent {
                    animator: animator.clone(),
                    event: name(NameKind::Event),
                    body: Vec::new(),
                }),
                R(Expression::AnimatorBool {
                    animator: animator.clone(),
                    parameter: name(NameKind::Bool),
                }),
                R(Expression::AnimatorNumber {
                    animator: animator.clone(),
                    parameter: name(NameKind::Number),
                }),
                R(Expression::AnimatorLayerWeight {
                    animator: animator.clone(),
                    layer: name(NameKind::Layer),
                }),
                R(Expression::Property {
                    target: animator,
                    property: "CurrentState".to_owned(),
                }),
            ]
        }
        Category::Tweens => {
            let gui = first(Filter::Gui);
            vec![
                block(BlockKind::Tween {
                    target: part.clone(),
                    property: "Position".to_owned(),
                    value: vector(0.0, 10.0, 0.0),
                    seconds: number(1.0),
                    style: "Quad".to_owned(),
                    direction: "Out".to_owned(),
                }),
                block(BlockKind::SlideIn {
                    target: gui.clone(),
                    from: "Left".to_owned(),
                    seconds: number(0.5),
                }),
                block(BlockKind::FadeIn {
                    target: gui.clone(),
                    seconds: number(0.5),
                }),
                block(BlockKind::FadeOut {
                    target: gui,
                    seconds: number(0.5),
                }),
            ]
        }
        Category::More => vec![
            block(BlockKind::Print {
                value: text("Hello!"),
            }),
            block(BlockKind::Destroy {
                target: first(Filter::Parts),
            }),
            block(BlockKind::CloneInto {
                target: first(Filter::Parts),
                parent: String::new(),
            }),
            block(BlockKind::SaveSlot { slot: number(1.0) }),
            block(BlockKind::LoadSlot { slot: number(1.0) }),
            block(BlockKind::SetCheckpoint {
                checkpoint: first(Filter::Checkpoints),
            }),
            block(BlockKind::RestoreCheckpoint),
            block(BlockKind::LoadLevel {
                level: text("Castle"),
                spawn: text("Gate"),
            }),
            block(BlockKind::AddLevel {
                level: text("Dungeon"),
            }),
            block(BlockKind::RemoveLevel {
                level: text("Dungeon"),
            }),
            block(BlockKind::HostGame {
                port: number(47800.0),
            }),
            block(BlockKind::JoinGame {
                address: text("192.168.1.20:47800"),
            }),
            block(BlockKind::LeaveGame),
            block(BlockKind::SetNetworkOwner {
                target: part.clone(),
                owner: Expression::Variable {
                    name: "player".to_owned(),
                },
            }),
            block(BlockKind::SetNetworkOwnershipAuto {
                target: part.clone(),
            }),
            block(BlockKind::SendChatMessage {
                text: text("Hello"),
            }),
            block(BlockKind::ShowSystemMessage {
                text: text("Round over"),
            }),
            block(BlockKind::WhenLayerChanged {
                data_layer: "Night".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::SetLayerActive {
                data_layer: "Night".to_owned(),
                active: Expression::Boolean { value: true },
            }),
            block(BlockKind::KickPlayer {
                player: Expression::Variable {
                    name: "player".to_owned(),
                },
                reason: text("Cheating"),
            }),
            block(BlockKind::WhenConnectionFails { body: Vec::new() }),
            block(BlockKind::RequestStreamAround {
                player: Expression::Variable {
                    name: "player".to_owned(),
                },
                position: Expression::Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 500.0,
                },
            }),
            block(BlockKind::WhenStreamedIn {
                target: part.clone(),
                body: Vec::new(),
            }),
            R(Expression::DataValue {
                column: storage_name(NameKind::Column),
                row: Box::new(Expression::DataRow {
                    table: storage_name(NameKind::DataTable),
                    key: Box::new(text("sword")),
                }),
            }),
            R(Expression::DataRow {
                table: storage_name(NameKind::DataTable),
                key: Box::new(text("sword")),
            }),
            R(Expression::LayerActive {
                data_layer: "Night".to_owned(),
            }),
        ],
        // Grey blocks only come from code, so there are none of those.
        Category::Lua => vec![
            block(BlockKind::Comment {
                text: "-- A note".to_owned(),
            }),
            block(BlockKind::CallFunction {
                function: name("print"),
                args: vec![text("Hello")],
            }),
            block(BlockKind::CallMethod {
                object: name("workspace"),
                method: "GetChildren".to_owned(),
                args: Vec::new(),
            }),
            R(Expression::Call {
                function: Box::new(name("tostring")),
                args: vec![number(1.0)],
            }),
            R(Expression::MethodCall {
                object: Box::new(name("workspace")),
                method: "WaitForChild".to_owned(),
                args: vec![text("Part")],
            }),
            R(Expression::Index {
                object: Box::new(name("list")),
                key: Box::new(number(1.0)),
            }),
            R(Expression::Table {
                fields: vec![TableField::Named {
                    name: "name".to_owned(),
                    value: number(1.0),
                }],
                multiline: false,
            }),
            R(Expression::Binary {
                op: "..".to_owned(),
                left: Box::new(text("a")),
                right: Box::new(text("b")),
            }),
            R(Expression::Unary {
                op: "#".to_owned(),
                value: Box::new(name("list")),
            }),
        ],
    };
    // The blocks for Luau itself come after each category's own.
    let mut luau = Vec::new();
    match category {
        Category::Events => luau.extend([
            block(BlockKind::RunScript { body: Vec::new() }),
            block(BlockKind::OnSignal {
                signal: name("workspace.ChildAdded"),
                method: "Connect".to_owned(),
                params: "child".to_owned(),
                body: Vec::new(),
            }),
            block(BlockKind::Connect {
                signal: name("workspace.ChildAdded"),
                method: "Connect".to_owned(),
                params: "child".to_owned(),
                body: Vec::new(),
            }),
        ]),
        Category::Control => luau.extend([
            block(BlockKind::DefineFunction {
                name: "myFunction".to_owned(),
                local: true,
                params: "value".to_owned(),
                generics: String::new(),
                returns: String::new(),
                attributes: String::new(),
                body: Vec::new(),
            }),
            block(BlockKind::While {
                condition: condition(),
                body: Vec::new(),
            }),
            block(BlockKind::RepeatLoop {
                body: Vec::new(),
                condition: condition(),
            }),
            block(BlockKind::NumericFor {
                variable: "i".to_owned(),
                start: number(1.0),
                stop: number(10.0),
                step: None,
                body: Vec::new(),
            }),
            block(BlockKind::GenericFor {
                names: "key, value".to_owned(),
                values: vec![name("list")],
                body: Vec::new(),
            }),
            block(BlockKind::DoBlock { body: Vec::new() }),
            block(BlockKind::Break),
            block(BlockKind::Continue),
            block(BlockKind::Return {
                values: vec![name("value")],
            }),
        ]),
        Category::Variables => luau.extend([
            block(BlockKind::Local {
                names: "name".to_owned(),
                values: vec![number(0.0)],
            }),
            block(BlockKind::Assign {
                targets: vec![name("name")],
                values: vec![number(0.0)],
            }),
            block(BlockKind::CompoundAssign {
                target: name("name"),
                op: "+".to_owned(),
                value: number(1.0),
            }),
        ]),
        _ => {}
    }
    // Keyboard and button blocks only work on the player's side.
    if !client {
        for list in [&mut items, &mut luau] {
            list.retain(|item| match item {
                PaletteItem::Block(kind) => !kind.client_only(),
                PaletteItem::Reporter(expression) => !expression.client_only(),
                PaletteItem::NewVariable => true,
            });
        }
    }
    (items, luau)
}

/// The height of the search box above the palette's blocks.
pub(super) const SEARCH_HEIGHT: f32 = 40.0;

/// What a palette item is found by when searching: the words on it, its
/// category and the Lua it becomes, in lower case.
pub(super) fn search_text(
    item: &PaletteItem,
    category: Category,
    objects: &[ObjectInfo],
) -> String {
    let mut words = vec![category.name().to_owned()];
    fn add_labels(words: &mut Vec<String>, parts: &[Part]) {
        for part in parts {
            if let Part::Label(text) = part {
                words.push(text.clone());
            }
        }
    }
    match item {
        PaletteItem::Block(kind) => {
            words.extend(item_lua(item));
            let mut kind = (**kind).clone();
            for section in sections(&mut kind, objects) {
                if let Section::Row(parts) = &section {
                    add_labels(&mut words, parts);
                }
            }
        }
        PaletteItem::Reporter(expression) => {
            words.extend(item_lua(item));
            let mut expression = expression.clone();
            add_labels(&mut words, &reporter_parts(&mut expression, objects));
        }
        PaletteItem::NewVariable => words.push("make a variable".to_owned()),
    }
    words.join(" ").to_lowercase()
}

/// Whether every word typed in the search box is in `text` (from
/// [`search_text`]).
pub(super) fn matches_search(text: &str, search: &str) -> bool {
    search
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

/// The Lua a palette block becomes, on its own.
fn block_lua(kind: &BlockKind) -> Option<String> {
    axcenity_blocks::block_lua(&Block::new(kind.clone()), true).ok()
}

/// The Lua a palette item becomes, on its own.
pub(super) fn item_lua(item: &PaletteItem) -> Option<String> {
    match item {
        PaletteItem::Block(kind) => block_lua(kind),
        PaletteItem::Reporter(expression) => axcenity_blocks::expression_lua(expression, true).ok(),
        PaletteItem::NewVariable => None,
    }
}

/// What the catalogue says about a palette or node menu item.
pub(super) fn item_info(item: &PaletteItem) -> Option<axcenity_blocks::catalogue::NodeInfo> {
    match item {
        PaletteItem::Block(kind) => Some(axcenity_blocks::catalogue::block_info(kind)),
        PaletteItem::Reporter(expression) => {
            Some(axcenity_blocks::catalogue::expression_info(expression))
        }
        PaletteItem::NewVariable => None,
    }
}

/// A palette, node menu or node tooltip (cohesion pass CW4.2): the
/// catalogue's card (what it does, its inputs, what it gives, an example),
/// `how` to use it, then the Lua it becomes, so a beginner sees what it
/// does and learns the Lua too.
pub(super) fn lua_hint(
    ui: &mut egui::Ui,
    how: &str,
    info: Option<axcenity_blocks::catalogue::NodeInfo>,
    lua: Option<String>,
) {
    match &info {
        Some(info) => crate::node_graph::node_card(ui, info, how),
        None => {
            ui.set_max_width(crate::widgets::INFO_CARD_WIDTH);
            ui.label(how);
        }
    }
    if let Some(lua) = lua {
        ui.label(
            egui::RichText::new("It becomes this Lua:")
                .size(theme::type_scale::SMALL)
                .color(theme::palette(ui.ctx()).text_weak),
        );
        let job = lua_syntax::layout_job(&lua, FontId::monospace(12.0), ui.visuals());
        ui.add(egui::Label::new(job));
    }
}

pub(super) fn show_palette(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut State,
    context: &BlockContext,
    variables: &[String],
) {
    let background = ui.visuals().faint_bg_color;
    ui.painter().rect_filled(rect, 0, background);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    // The search box, then the blocks under it.
    let search_rect = Rect::from_min_size(rect.min, vec2(rect.width(), SEARCH_HEIGHT));
    let mut search_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(search_rect.shrink2(vec2(12.0, 8.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let search_box = search_ui
        .add(
            egui::TextEdit::singleline(&mut state.search)
                .id(egui::Id::new(("block search", context.key)))
                .hint_text(theme::with_icon(icons::MAGNIFYING_GLASS, "Search blocks"))
                .desired_width(f32::INFINITY),
        )
        .on_hover_text(
            "Finds blocks by their words or the Lua they become, such as \"sound\" or \"Destroy\"",
        );
    if search_box.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.search.clear();
    }
    let list_rect = Rect::from_min_max(pos2(rect.left(), search_rect.bottom()), rect.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
    child.set_clip_rect(list_rect);
    let scroll_to = state.scroll_to.take();
    let mut new_variable = state.new_variable.take();
    let mut made = None;
    let mut started = None;
    let search = state.search.trim().to_owned();
    egui::ScrollArea::vertical()
        .id_salt(("block palette", context.key, search.is_empty()))
        .auto_shrink(false)
        .show(&mut child, |ui| {
            ui.add_space(8.0);
            let mut found = false;
            for category in Category::ALL {
                let (mut items, mut luau) =
                    palette_parts(category, context.client, context.objects, variables);
                if !search.is_empty() {
                    for list in [&mut items, &mut luau] {
                        list.retain(|item| {
                            matches_search(&search_text(item, category, context.objects), &search)
                        });
                    }
                    if items.is_empty() && luau.is_empty() {
                        continue;
                    }
                    found = true;
                }
                // The Luau blocks under their own heading, folded unless
                // opened or searched for.
                let mut rows: Vec<Option<PaletteItem>> = items.into_iter().map(Some).collect();
                if !luau.is_empty() {
                    rows.push(None);
                    if !search.is_empty() || state.luau_open.contains(&category) {
                        rows.extend(luau.into_iter().map(Some));
                    }
                }
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    // The category list's swatch, small.
                    let (swatch, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    ui.painter()
                        .rect_filled(swatch, 5, category.fill(&theme::palette(ui.ctx())));
                    ui.painter().text(
                        swatch.center(),
                        egui::Align2::CENTER_CENTER,
                        category.icon(),
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                    let heading = ui.label(
                        egui::RichText::new(category.name())
                            .strong()
                            .family(theme::heading_family()),
                    );
                    if scroll_to == Some(category) {
                        heading.scroll_to_me(Some(egui::Align::TOP));
                    }
                });
                ui.add_space(6.0);
                for row in rows {
                    let Some(item) = row else {
                        if luau_heading(ui, category, state, !search.is_empty()) {
                            if let Some(at) =
                                state.luau_open.iter().position(|open| *open == category)
                            {
                                state.luau_open.remove(at);
                            } else {
                                state.luau_open.push(category);
                            }
                        }
                        ui.add_space(8.0);
                        continue;
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        match item {
                            PaletteItem::NewVariable => {
                                if let Some(name) = &mut new_variable {
                                    let edit = ui.add(
                                        egui::TextEdit::singleline(name)
                                            .hint_text("Variable name")
                                            .desired_width(150.0),
                                    );
                                    edit.request_focus();
                                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if enter || ui.button("OK").clicked() {
                                        made = Some(name.clone());
                                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        made = Some(String::new());
                                    }
                                } else if ui
                                    .button("Make a Variable")
                                    .on_hover_text(
                                        "Names a value the whole script can read and change, \
                                         such as a score. Its blocks appear below.",
                                    )
                                    .clicked()
                                {
                                    new_variable = Some(String::new());
                                }
                            }
                            PaletteItem::Block(kind) => {
                                if let Some(drag) =
                                    palette_block(ui, *kind, context, variables, background)
                                {
                                    started = Some(drag);
                                }
                            }
                            PaletteItem::Reporter(expression) => {
                                if let Some(drag) =
                                    palette_reporter(ui, expression, context, variables)
                                {
                                    started = Some(drag);
                                }
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                ui.add_space(14.0);
            }
            if !search.is_empty() && !found {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    ui.weak(format!(
                        "No blocks match \"{search}\". Try another word, or a word from the Lua."
                    ));
                });
            }
        });
    if let Some(name) = made {
        new_variable = None;
        let name = name.trim().to_owned();
        if valid_identifier(&name) && !variables.contains(&name) {
            state.variables.push(name);
        }
    }
    state.new_variable = new_variable;
    if started.is_some() {
        state.drag = started;
    }
}

/// The heading over a category's Luau blocks, which opens and folds them.
/// Returns true if it was clicked.
fn luau_heading(ui: &mut egui::Ui, category: Category, state: &State, searching: bool) -> bool {
    let open = searching || state.luau_open.contains(&category);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let arrow = if open {
            icons::CARET_DOWN
        } else {
            icons::CARET_RIGHT
        };
        let response = ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("{arrow} Lua blocks"))
                        .size(theme::type_scale::SMALL),
                )
                .frame(false),
            )
            .on_hover_text(
                "Blocks for Lua itself: local, while, for, a function, a :Connect. \
                 They write exactly the Lua they say. The blocks above do the common \
                 things more simply; these are for scripts switched from Code, or \
                 when you know the Lua you want.",
            );
        // While searching they're shown anyway.
        response.clicked() && !searching
    })
    .inner
}

/// A block in the palette, which can be dragged out. Returns the drag if
/// one started.
pub(super) fn palette_block(
    ui: &mut egui::Ui,
    kind: BlockKind,
    context: &BlockContext,
    variables: &[String],
    background: Color32,
) -> Option<Drag> {
    let event = kind.is_event();
    let mut block = Block::new(kind.clone());
    // Measured with a painter that draws nothing, then drawn where it fits.
    let measure = ui.painter().with_clip_rect(Rect::NOTHING);
    let mut painter = BlockPainter::new(
        measure,
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    let size = if event {
        let drawn = painter.hat(ui, &mut block, Pos2::ZERO);
        vec2(drawn.width, drawn.height + TAB_DEPTH)
    } else {
        let drawn = painter.statement(ui, &mut block, Pos2::ZERO, true);
        vec2(drawn.width, drawn.height + TAB_DEPTH)
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    let response = response
        .on_hover_cursor(CursorIcon::Grab)
        .on_hover_ui(|ui| {
            lua_hint(
                ui,
                "Drag it onto the canvas.",
                Some(axcenity_blocks::catalogue::block_info(&kind)),
                block_lua(&kind),
            )
        });
    let mut painter = BlockPainter::new(
        ui.painter().clone(),
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    if event {
        painter.hat(ui, &mut block, rect.min);
    } else {
        painter.statement(ui, &mut block, rect.min, true);
    }
    painter.finish();
    if response.drag_started()
        && let Some(origin) = response.interact_pointer_pos()
    {
        return Some(Drag {
            item: Dragged::Stack(vec![Block::new(kind)]),
            grab: origin - rect.min,
            size,
        });
    }
    None
}

pub(super) fn palette_reporter(
    ui: &mut egui::Ui,
    mut expression: Expression,
    context: &BlockContext,
    variables: &[String],
) -> Option<Drag> {
    let background = ui.visuals().faint_bg_color;
    let painter = BlockPainter::new(
        ui.painter().clone(),
        1.0,
        false,
        background,
        egui::Id::NULL,
        context,
        variables,
    );
    let size = painter.reporter_size(ui, &mut expression);
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    let response = response
        .on_hover_cursor(CursorIcon::Grab)
        .on_hover_ui(|ui| {
            lua_hint(
                ui,
                "Drag it into a slot of the same shape.",
                Some(axcenity_blocks::catalogue::expression_info(&expression)),
                axcenity_blocks::expression_lua(&expression, true).ok(),
            )
        });
    let mut painter = painter;
    painter.reporter(ui, &mut expression.clone(), rect.min, egui::Id::NULL, None);
    if response.drag_started()
        && let Some(origin) = response.interact_pointer_pos()
    {
        return Some(Drag {
            item: Dragged::Reporter(expression),
            grab: origin - rect.min,
            size,
        });
    }
    None
}
