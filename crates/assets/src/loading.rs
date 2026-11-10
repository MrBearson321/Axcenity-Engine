//! Loading files without holding the lock, and without waiting (roadmap M5).
//!
//! Every file goes through [`Assets::load`]: it notes under the lock that
//! the file is being read, then reads and decodes with the lock released,
//! and takes the lock again only to keep the result. So two files decode at
//! once, and asking for something already loaded never waits for a decode.
//! A file is read once however many threads ask: the others wait on its
//! [`LoadTicket`], not on the lock.
//!
//! The non-blocking way in is `request_*` (or [`Assets::request`], which
//! goes by the file's extension). It queues the file for the worker
//! threads ([`load_workers`] of them at most) and returns a ticket to poll or wait on;
//! [`Assets::model_ready`] then hands over the model without ever loading it.
//! A blocking call for a file still in the queue reads it there and then,
//! rather than wait behind the queue.
//!
//! A file that changes while it's being read (`forget`, `reload`) is read
//! again before anything is kept, and a value handed over while it's being
//! read (`insert`, `put_*`) wins, so a slow load can't bring back old data.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Condvar, LazyLock};

use super::*;

/// How many files are read at once in the background at most: every core
/// but two, which are left for drawing, and at least four and at most eight.
pub(crate) fn load_workers() -> usize {
    static WORKERS: LazyLock<usize> = LazyLock::new(|| {
        std::thread::available_parallelism()
            .map_or(4, |cores| cores.get().saturating_sub(2).clamp(4, 8))
    });
    *WORKERS
}

/// Runs off the lock just before each file is read, with its path. Tests
/// use it to hold a load open at a known point.
pub(crate) type LoadGate = Arc<dyn Fn(&str) + Send + Sync>;

/// How a load is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    /// Queued, or being read.
    Loading,
    /// Loaded, and kept by the [`Assets`].
    Ready,
    /// It couldn't be loaded; why goes to
    /// [`take_errors`](Assets::take_errors), once.
    Failed,
}

/// How a model stands for a frame being drawn, from
/// [`model_slot`](Assets::model_slot).
#[derive(Clone, Debug)]
pub enum Slot<T> {
    Ready(Arc<T>),
    /// On its way: draw nothing for it this frame.
    Loading,
    /// It has no file, or the file can't be loaded.
    Failed,
}

/// A load in progress, from a `request_*` call. Cloning gives another handle
/// to the same load.
#[derive(Clone, Debug)]
pub struct LoadTicket(Arc<Ticket>);

#[derive(Debug)]
struct Ticket {
    status: Mutex<LoadStatus>,
    finished: Condvar,
}

impl LoadTicket {
    pub(crate) fn new(status: LoadStatus) -> Self {
        Self(Arc::new(Ticket {
            status: Mutex::new(status),
            finished: Condvar::new(),
        }))
    }

    /// How the load is going now. Never waits.
    pub fn status(&self) -> LoadStatus {
        *self.status_guard()
    }

    /// Whether the load is over, one way or the other. Never waits.
    pub fn is_finished(&self) -> bool {
        self.status() != LoadStatus::Loading
    }

    /// Waits for the load to be over, and says how it went.
    pub fn wait(&self) -> LoadStatus {
        let mut status = self.status_guard();
        while *status == LoadStatus::Loading {
            status = self
                .0
                .finished
                .wait(status)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *status
    }

    /// Ends the load and wakes whoever waits on it. The first call decides.
    fn finish(&self, outcome: LoadStatus) {
        let mut status = self.status_guard();
        if *status == LoadStatus::Loading {
            *status = outcome;
            self.0.finished.notify_all();
        }
    }

    fn same_load(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn status_guard(&self) -> std::sync::MutexGuard<'_, LoadStatus> {
        self.0.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What kind of thing a file is loaded as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Model,
    Texture,
    SkyImage,
    ReflectionProbe,
    LightProbes,
    ColorLookup,
    Palette,
    Takes,
    DataTable,
    Fonts,
    /// A material file put together with its bases.
    Material,
    /// A material as above, then every texture it uses.
    MaterialAndMaps,
    /// Any other project file: nothing is kept, it only has to be there.
    File,
}

impl Kind {
    /// What the file at `path` is loaded as, going by its name.
    fn of(path: &str) -> Self {
        let file = path.split('#').next().unwrap_or(path);
        let extension = file
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        if ModelFormat::of(Path::new(file)).is_some() {
            Self::Model
        } else if is_material_file(file) {
            Self::MaterialAndMaps
        } else if is_light_probes_file(file) {
            Self::LightProbes
        } else if is_probe_file(file) {
            Self::ReflectionProbe
        } else if is_data_table(file) {
            Self::DataTable
        } else if is_hdr_sky_file(file) {
            Self::SkyImage
        } else if extension == "cube" {
            Self::ColorLookup
        } else if TEXTURE_EXTENSIONS.contains(&extension.as_str()) {
            Self::Texture
        } else {
            Self::File
        }
    }

    /// Whether a file of this kind is left alone by a reload when its size
    /// and modification time haven't changed.
    fn stamped(self) -> bool {
        !matches!(
            self,
            Self::Material | Self::MaterialAndMaps | Self::Fonts | Self::File
        )
    }

    /// Whether [`TextureProgress`] counts it.
    pub(crate) fn is_image(self) -> bool {
        matches!(self, Self::Texture | Self::SkyImage)
    }
}

/// What's being loaded: the same file can be two kinds of thing (a PNG is a
/// texture, a sky image or a lookup table).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LoadKey {
    pub(crate) kind: Kind,
    path: String,
}

impl LoadKey {
    pub(crate) fn new(kind: Kind, path: &str) -> Self {
        Self {
            kind,
            path: path.to_owned(),
        }
    }

    /// The file it reads: its path without a model's `#` pick.
    pub(crate) fn file(&self) -> &str {
        self.path.split('#').next().unwrap_or(&self.path)
    }
}

/// One file queued or being read.
pub(crate) struct Load {
    ticket: LoadTicket,
    /// Still waiting for a worker; anyone who needs it now can take it.
    pub(crate) queued: bool,
    /// The file changed after this read began, so it's read again.
    reread: bool,
    /// Came through the queue, so the progress counts include it.
    pub(crate) background: bool,
}

/// Files asked for since none were loading.
#[derive(Default)]
pub(crate) struct Batch {
    total: usize,
    loading: usize,
}

impl Batch {
    fn add(&mut self) {
        self.total += 1;
        self.loading += 1;
    }

    fn finish_one(&mut self) {
        self.loading -= 1;
        if self.loading == 0 {
            self.total = 0;
        }
    }

    pub(crate) fn done(&self) -> usize {
        self.total - self.loading
    }

    pub(crate) fn total(&self) -> usize {
        self.total
    }
}

/// What a load gives: whether it counts as loaded, for its ticket.
pub(crate) trait Loaded {
    fn loaded(&self) -> bool;
}

impl<T> Loaded for Option<T> {
    fn loaded(&self) -> bool {
        self.is_some()
    }
}

impl<T, E> Loaded for Result<T, E> {
    fn loaded(&self) -> bool {
        self.is_ok()
    }
}

impl Loaded for bool {
    fn loaded(&self) -> bool {
        *self
    }
}

/// Takes `key` off the list of loads, keeping the progress counts right.
fn end_load(shared: &mut Shared, key: &LoadKey) -> Option<Load> {
    let load = shared.loads.remove(key)?;
    if load.background {
        shared.jobs.finish_one();
        if key.kind.is_image() {
            shared.images.finish_one();
        }
    }
    Some(load)
}

/// A value for `key` was just handed over: any load of it is dropped, and
/// whoever waits on one finds the value.
pub(crate) fn replace_load(shared: &mut Shared, key: &LoadKey) {
    shared.generation += 1;
    if let Some(load) = end_load(shared, key) {
        load.ticket.finish(LoadStatus::Ready);
    }
}

/// Files matching `changed` are out of date: a read already under way is
/// done again before anything is kept.
pub(crate) fn reread_loads(shared: &mut Shared, changed: impl Fn(&LoadKey) -> bool) {
    for (key, load) in &mut shared.loads {
        if !load.queued && changed(key) {
            load.reread = true;
        }
    }
}

/// The model files being read now, for a reload of a file one may read.
pub(crate) fn models_being_read(shared: &Shared) -> impl Iterator<Item = &str> {
    shared
        .loads
        .keys()
        .filter(|key| key.kind == Kind::Model)
        .map(|key| key.path.as_str())
}

impl Assets {
    /// Loads one file. Under the lock it looks for the result with `cached`
    /// and otherwise claims the load; `read` then runs with the lock
    /// released; `store` keeps what it gave, under the lock. `no_project`
    /// is the answer when no project is open.
    pub(crate) fn load<T: Loaded, R>(
        &self,
        kind: Kind,
        path: &str,
        no_project: T,
        cached: impl Fn(&Shared) -> Option<T>,
        read: impl Fn(&Path) -> Result<R, String>,
        store: impl FnOnce(&mut Shared, Result<R, String>) -> T,
    ) -> T {
        // Made only on a miss: this path is called for every part, every frame.
        let mut key = None;
        loop {
            let claimed = {
                let mut shared = self.lock();
                if let Some(known) = cached(&shared) {
                    return known;
                }
                let Some(folder) = shared.folder.clone() else {
                    return no_project;
                };
                let key = key.get_or_insert_with(|| LoadKey::new(kind, path));
                let gate = shared.gate.clone();
                match shared.loads.get_mut(key) {
                    // Being read: wait for that, then look again.
                    Some(load) if !load.queued => Err(load.ticket.clone()),
                    // Waiting for a worker: read it here instead.
                    Some(load) => {
                        load.queued = false;
                        Ok((load.ticket.clone(), folder, gate))
                    }
                    None => {
                        let ticket = LoadTicket::new(LoadStatus::Loading);
                        shared.loads.insert(
                            key.clone(),
                            Load {
                                ticket: ticket.clone(),
                                queued: false,
                                reread: false,
                                background: false,
                            },
                        );
                        Ok((ticket, folder, gate))
                    }
                }
            };
            let (ticket, folder, gate) = match claimed {
                Ok(claimed) => claimed,
                Err(other_load) => {
                    other_load.wait();
                    continue;
                }
            };
            let key = key.as_ref().expect("made above");
            loop {
                let stamp = kind.stamped().then(|| reload::stamp(&folder, key.file()));
                // A decoder that panics fails this file, and must not leave
                // it "loading" for ever with others waiting on it.
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    if let Some(gate) = &gate {
                        gate(path);
                    }
                    read(&folder)
                }))
                .unwrap_or_else(|_| Err("the decoder failed on it".to_owned()));

                let mut shared = self.lock();
                match shared.loads.get_mut(key) {
                    Some(load) if load.ticket.same_load(&ticket) => {
                        if load.reread {
                            load.reread = false;
                            continue;
                        }
                    }
                    // Something newer was handed over while this was read.
                    _ => {
                        drop(shared);
                        ticket.finish(LoadStatus::Ready);
                        break;
                    }
                }
                if let Some(stamp) = stamp {
                    reload::keep_stamp(&mut shared, key.file(), stamp);
                }
                let value = store(&mut shared, result);
                shared.generation += 1;
                end_load(&mut shared, key);
                drop(shared);
                ticket.finish(if value.loaded() {
                    LoadStatus::Ready
                } else {
                    LoadStatus::Failed
                });
                return value;
            }
        }
    }

    /// Starts loading the model at `mesh_id` on a background thread, unless
    /// it's loaded or loading already, and returns at once. A failed load is
    /// remembered, as for [`model_blocking`](Self::model_blocking).
    pub fn request_model(&self, mesh_id: &str) -> LoadTicket {
        self.request_kind(Kind::Model, mesh_id)
    }

    /// As [`request_model`](Self::request_model), for a texture file.
    pub fn request_texture(&self, path: &str) -> LoadTicket {
        self.request_kind(Kind::Texture, path)
    }

    /// As [`request_model`](Self::request_model), for a sky image.
    pub fn request_sky_image(&self, path: &str) -> LoadTicket {
        self.request_kind(Kind::SkyImage, path)
    }

    /// As [`request_model`](Self::request_model), for a material file and
    /// every texture it uses. The ticket finishes when they're all in, and
    /// fails if the material or any of its textures can't be read.
    pub fn request_material(&self, path: &str) -> LoadTicket {
        self.request_kind(Kind::MaterialAndMaps, path)
    }

    /// As [`request_model`](Self::request_model), for any project file,
    /// loaded as what its name says it is: a model, a material with its
    /// textures, a texture, a probe bake, an HDR sky image, a lookup table
    /// or a data table. Any other file (a sound, say) isn't kept in memory,
    /// so it only has to be there.
    pub fn request(&self, path: &str) -> LoadTicket {
        self.request_kind(Kind::of(path), path)
    }

    fn request_kind(&self, kind: Kind, path: &str) -> LoadTicket {
        if path.is_empty() {
            return LoadTicket::new(LoadStatus::Failed);
        }
        let mut shared = self.lock();
        if shared.folder.is_none() {
            return LoadTicket::new(LoadStatus::Failed);
        }
        if let Some(known) = known_status(&shared, kind, path) {
            return LoadTicket::new(known);
        }
        self.enqueue(&mut shared, LoadKey::new(kind, path))
    }

    /// The model at `mesh_id` if it's loaded, or `None` if it isn't (yet),
    /// or couldn't be. Never loads anything and never waits for a load.
    pub fn model_ready(&self, mesh_id: &str) -> Option<Arc<Model>> {
        self.lock().models.get(mesh_id).cloned().flatten()
    }

    /// The model at `mesh_id` for a frame being drawn. After
    /// [`load_textures_in_background`](Self::load_textures_in_background)
    /// this never waits for a file: one not read yet is queued for a worker
    /// and is [`Slot::Loading`] until it's in, so a big model can't stall a
    /// frame. Otherwise it's read before this returns, like
    /// [`model_blocking`](Self::model_blocking).
    pub fn model_slot(&self, mesh_id: &str) -> Slot<Model> {
        if mesh_id.is_empty() {
            return Slot::Failed;
        }
        {
            let mut shared = self.lock();
            match shared.models.get(mesh_id) {
                Some(Some(model)) => return Slot::Ready(model.clone()),
                Some(None) => return Slot::Failed,
                None => {}
            }
            if shared.folder.is_none() {
                return Slot::Failed;
            }
            if shared.waker.is_some() {
                self.enqueue(&mut shared, LoadKey::new(Kind::Model, mesh_id));
                return Slot::Loading;
            }
        }
        match self.model_blocking(mesh_id) {
            Some(model) => Slot::Ready(model),
            None => Slot::Failed,
        }
    }

    /// Of the files queued since none were loading, how many are done and
    /// how many there are: `(0, 0)` when nothing is queued or being read in
    /// the background.
    pub fn progress(&self) -> (usize, usize) {
        let shared = self.lock();
        (shared.jobs.done(), shared.jobs.total())
    }

    /// A number that goes up whenever something new is loaded or handed
    /// over, so a host can tell whether to look again.
    pub fn generation(&self) -> u64 {
        self.lock().generation
    }

    /// Queues `key` for a worker, starting one if fewer than
    /// [`load_workers`] are running. If it's queued or being read already,
    /// that load's ticket is returned.
    pub(crate) fn enqueue(&self, shared: &mut Shared, key: LoadKey) -> LoadTicket {
        if let Some(load) = shared.loads.get(&key) {
            return load.ticket.clone();
        }
        let ticket = LoadTicket::new(LoadStatus::Loading);
        shared.jobs.add();
        if key.kind.is_image() {
            shared.images.add();
        }
        shared.loads.insert(
            key.clone(),
            Load {
                ticket: ticket.clone(),
                queued: true,
                reread: false,
                background: true,
            },
        );
        shared.queue.push_back(key);
        if shared.workers < load_workers() {
            shared.workers += 1;
            let assets = self.clone();
            std::thread::Builder::new()
                .name("asset loader".to_owned())
                .spawn(move || assets.work())
                .expect("starting a thread only fails when out of resources");
        }
        ticket
    }

    /// A worker: loads queued files until there are none left.
    fn work(&self) {
        loop {
            let (key, waker) = {
                let mut shared = self.lock();
                let Some(key) = next_job(&mut shared) else {
                    shared.workers -= 1;
                    return;
                };
                (key, shared.waker.clone())
            };
            self.run(&key);
            if let Some(waker) = waker {
                waker();
            }
        }
    }

    /// Loads `key` on this thread, the way a blocking call would.
    fn run(&self, key: &LoadKey) {
        let path = key.path.as_str();
        match key.kind {
            Kind::Model => drop(self.model_blocking(path)),
            Kind::Texture => drop(self.texture_blocking(path)),
            Kind::SkyImage => drop(self.sky_image_blocking(path)),
            Kind::ReflectionProbe => drop(self.reflection_probe(path)),
            Kind::LightProbes => drop(self.light_probes(path)),
            Kind::ColorLookup => drop(self.color_lookup(path)),
            Kind::Palette => drop(self.palette(path)),
            Kind::Takes => drop(self.takes(path)),
            Kind::DataTable => {
                // A script reading the table gets this as its error; a load
                // ahead of time has nowhere else to say it.
                if let Err(why) = self.table_at(path.to_owned()) {
                    let error = format!("Could not load the data table `{path}`: {why}");
                    self.lock().errors.push(error);
                }
            }
            Kind::Fonts => drop(self.fonts()),
            Kind::Material => drop(self.material(path)),
            Kind::MaterialAndMaps => drop(self.material_and_maps(path)),
            Kind::File => drop(self.check_file(path)),
        }
    }

    /// Loads the material at `path` and every texture it uses, the textures
    /// on the workers as far as there are any free. False if any can't be.
    fn material_and_maps(&self, path: &str) -> bool {
        self.load(
            Kind::MaterialAndMaps,
            path,
            false,
            |_| None,
            |_| {
                let Some(material) = self.material(path) else {
                    return Ok(false);
                };
                let maps: Vec<&str> = material.image_paths().collect();
                for map in &maps {
                    self.request_texture(map);
                }
                // Not `all`, which would stop at the first that failed.
                let loaded = maps
                    .iter()
                    .filter(|map| self.texture_blocking(map).is_some())
                    .count();
                Ok(loaded == maps.len())
            },
            |_, loaded| loaded.unwrap_or(false),
        )
    }

    /// Whether the project file at `path` is there, for a file with nothing
    /// to keep in memory.
    fn check_file(&self, path: &str) -> bool {
        self.load(
            Kind::File,
            path,
            false,
            |_| None,
            |folder| {
                let relative = safe_path(path)
                    .ok_or("asset paths must name a file inside the project folder")?;
                if folder.join(relative).is_file() {
                    Ok(())
                } else {
                    Err("the file isn't there".to_owned())
                }
            },
            |shared, found| match found {
                Ok(()) => true,
                Err(message) => {
                    let error = format!("Could not load `{path}`: {message}");
                    if !shared.errors.contains(&error) {
                        shared.errors.push(error);
                    }
                    false
                }
            },
        )
    }

    /// Runs `gate` off the lock just before each file is read, with its
    /// path: a test can hold a load open at a known point, or see which
    /// loads overlap, without sleeping. For tests only.
    #[doc(hidden)]
    pub fn set_load_gate(&self, gate: impl Fn(&str) + Send + Sync + 'static) {
        self.lock().gate = Some(Arc::new(gate));
    }
}

/// The next queued file still waiting for a worker. A file someone took
/// back out of the queue to read for themselves stays listed, and is skipped.
fn next_job(shared: &mut Shared) -> Option<LoadKey> {
    while let Some(key) = shared.queue.pop_front() {
        if shared.loads.get(&key).is_some_and(|load| load.queued) {
            return Some(key);
        }
    }
    None
}

/// How `path` has already gone, if it's been loaded (or failed) as `kind`.
fn known_status(shared: &Shared, kind: Kind, path: &str) -> Option<LoadStatus> {
    let of = |loaded: bool| {
        if loaded {
            LoadStatus::Ready
        } else {
            LoadStatus::Failed
        }
    };
    match kind {
        Kind::Model => shared.models.get(path).map(|found| of(found.is_some())),
        Kind::Texture => shared.textures.get(path).map(|found| of(found.is_some())),
        Kind::SkyImage => shared.sky_images.get(path).map(|found| of(found.is_some())),
        Kind::ReflectionProbe => shared.probes.get(path).map(|found| of(found.is_some())),
        Kind::LightProbes => shared
            .light_probes
            .get(path)
            .map(|found| of(found.is_some())),
        Kind::ColorLookup => shared.lookups.get(path).map(|found| of(found.is_some())),
        Kind::Palette => shared.palettes.get(path).map(|found| of(found.is_some())),
        Kind::Takes => shared.takes.get(path).map(|found| of(found.is_some())),
        Kind::DataTable => shared.tables.get(path).map(|found| of(found.is_ok())),
        Kind::Material => shared.materials.get(path).map(|found| of(found.is_some())),
        Kind::Fonts | Kind::MaterialAndMaps | Kind::File => None,
    }
}

#[cfg(test)]
#[path = "loading_tests.rs"]
mod tests;
