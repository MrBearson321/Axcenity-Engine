//! Tests for loading off the lock (roadmap M5). None of them rely on timing
//! to pass: each holds a load open at a known point with the load gate. The
//! time limits only turn a load that never comes into a failed test.

use std::collections::HashSet;
use std::fs;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use super::*;

/// How long a test waits for something that should happen right away.
const LIMIT: Duration = Duration::from_secs(30);

fn project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-loading-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("assets/models")).unwrap();
    fs::create_dir_all(folder.join("assets/textures")).unwrap();
    fs::create_dir_all(folder.join("assets/materials")).unwrap();
    folder
}

/// A one-triangle glTF `width` studs wide at `path`, its buffer beside it.
fn write_triangle(folder: &Path, path: &str, width: f32) {
    let corners: [f32; 9] = [0.0, 0.0, 0.0, width, 0.0, 0.0, 0.0, 1.0, 0.0];
    let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
    let buffer = format!("{path}.bin");
    let uri = buffer.rsplit('/').next().unwrap();
    let gltf = format!(
        r#"{{
            "asset": {{ "version": "2.0" }},
            "buffers": [ {{ "uri": "{uri}", "byteLength": 36 }} ],
            "bufferViews": [ {{ "buffer": 0, "byteLength": 36 }} ],
            "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 3,
                              "type": "VEC3", "min": [0, 0, 0], "max": [{width}, 1, 0] }} ],
            "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ]
        }}"#
    );
    fs::write(folder.join(&buffer), &bytes).unwrap();
    fs::write(folder.join(path), gltf).unwrap();
}

const A: &str = "assets/models/a.gltf";
const B: &str = "assets/models/b.gltf";
const C: &str = "assets/models/c.gltf";

fn project_with_models(name: &str) -> PathBuf {
    let folder = project(name);
    for path in [A, B, C] {
        write_triangle(&folder, path, 1.0);
    }
    folder
}

/// Each load through the gate waits until `parties` loads are in it at once.
/// If they run one at a time the first waits alone, runs out of `LIMIT` and
/// panics, which fails that file, and the test's asserts catch it.
fn gate_until_together(assets: &Assets, parties: usize) {
    let inside = Arc::new((Mutex::new(0_usize), std::sync::Condvar::new()));
    assets.set_load_gate(move |_| {
        let (count, changed) = &*inside;
        let mut count = count.lock().unwrap();
        *count += 1;
        changed.notify_all();
        let (count, timeout) = changed
            .wait_timeout_while(count, LIMIT, |count| *count < parties)
            .unwrap();
        assert!(
            !timeout.timed_out() || *count >= parties,
            "only {} of {parties} loads overlapped",
            *count
        );
    });
}

/// Holds every load of `held` until the returned sender is dropped or sent
/// to, and says on the receiver when one has got that far.
fn gate_hold(assets: &Assets, held: &'static str) -> (Sender<()>, mpsc::Receiver<()>) {
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let (reached, reaches) = mpsc::channel();
    let reached = Mutex::new(reached);
    assets.set_load_gate(move |path| {
        if path == held {
            let _ = reached.lock().unwrap().send(());
            let _ = released.lock().unwrap().recv_timeout(LIMIT);
        }
    });
    (release, reaches)
}

#[test]
fn two_requests_decode_at_the_same_time() {
    let folder = project_with_models("together");
    let assets = Assets::new(&folder);
    gate_until_together(&assets, 2);
    let a = assets.request_model(A);
    let b = assets.request_model(B);
    assert_eq!(a.wait(), LoadStatus::Ready);
    assert_eq!(b.wait(), LoadStatus::Ready);
    assert!(assets.take_errors().is_empty());
}

#[test]
fn two_blocking_loads_decode_at_the_same_time() {
    let folder = project_with_models("blocking-together");
    let assets = Assets::new(&folder);
    gate_until_together(&assets, 2);
    let other = assets.clone();
    let thread = std::thread::spawn(move || other.model_blocking(A).is_some());
    assert!(assets.model_blocking(B).is_some());
    assert!(thread.join().unwrap());
    assert!(assets.take_errors().is_empty());
}

#[test]
fn a_texture_and_a_model_decode_at_the_same_time() {
    let folder = project_with_models("mixed");
    fs::write(folder.join("assets/textures/a.png"), model::checker_png()).unwrap();
    let assets = Assets::new(&folder);
    gate_until_together(&assets, 2);
    let other = assets.clone();
    let thread = std::thread::spawn(move || other.texture("assets/textures/a.png").is_some());
    assert!(assets.model_blocking(A).is_some());
    assert!(thread.join().unwrap());
}

#[test]
fn the_lock_is_free_while_a_file_decodes() {
    let folder = project_with_models("lock-free");
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, A);
    let ticket = assets.request_model(A);
    reached.recv_timeout(LIMIT).expect("the load started");

    // Each of these needs the lock: with the decode holding it, they'd wait
    // for a load that waits for `release`, below them.
    assert_eq!(assets.folder().as_deref(), Some(folder.as_path()));
    assert!(assets.take_errors().is_empty());
    assert!(assets.model_ready(A).is_none());
    assert_eq!(ticket.status(), LoadStatus::Loading);
    assert!(
        assets.model_blocking(B).is_some(),
        "another file still loads"
    );

    release.send(()).unwrap();
    assert_eq!(ticket.wait(), LoadStatus::Ready);
    assert!(assets.model_ready(A).is_some());
}

#[test]
fn a_ticket_says_how_the_load_went_and_the_model_is_then_ready() {
    let folder = project_with_models("ticket");
    let assets = Assets::new(&folder);
    let before = assets.generation();
    assert!(assets.model_ready(A).is_none());
    assert_eq!(assets.progress(), (0, 0));

    let ticket = assets.request_model(A);
    assert_eq!(ticket.wait(), LoadStatus::Ready);
    assert!(ticket.is_finished());
    let model = assets.model_ready(A).expect("it's kept");
    assert!(assets.generation() > before);
    assert_eq!(assets.progress(), (0, 0));

    // Asking again doesn't load it again.
    let again = assets.request_model(A);
    assert_eq!(again.status(), LoadStatus::Ready);
    assert!(Arc::ptr_eq(&model, &assets.model_blocking(A).unwrap()));

    let gone = assets.request_model("assets/models/gone.glb");
    assert_eq!(gone.wait(), LoadStatus::Failed);
    let errors = assets.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("gone.glb"), "{errors:?}");
    let repeat = assets.request_model("assets/models/gone.glb");
    assert_eq!(repeat.status(), LoadStatus::Failed, "remembered");
    assert!(assets.take_errors().is_empty(), "and reported once");
}

#[test]
fn requests_for_nothing_fail_at_once() {
    let folder = project("nothing");
    assert_eq!(
        Assets::new(&folder).request_model("").status(),
        LoadStatus::Failed
    );
    assert_eq!(
        Assets::default().request_model(A).status(),
        LoadStatus::Failed
    );
}

#[test]
fn a_file_is_read_once_however_many_ask() {
    let folder = project_with_models("once");
    let assets = Assets::new(&folder);
    let reads = Arc::new(Mutex::new(Vec::new()));
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let (reached, reaches) = mpsc::channel();
    let reached = Mutex::new(reached);
    let log = reads.clone();
    assets.set_load_gate(move |path| {
        log.lock().unwrap().push(path.to_owned());
        reached.lock().unwrap().send(()).unwrap();
        let _ = released.lock().unwrap().recv_timeout(LIMIT);
    });

    let first = assets.request_model(A);
    reaches.recv_timeout(LIMIT).expect("the load started");
    let second = assets.request_model(A);
    assert_eq!(second.status(), LoadStatus::Loading, "joins the same load");
    let waiters: Vec<_> = (0..3)
        .map(|_| {
            let assets = assets.clone();
            std::thread::spawn(move || assets.model_blocking(A).unwrap())
        })
        .collect();
    release.send(()).unwrap();
    assert_eq!(second.wait(), LoadStatus::Ready);
    first.wait();
    let models: Vec<_> = waiters.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(*reads.lock().unwrap(), [A], "read once");
    assert!(models.iter().all(|m| Arc::ptr_eq(m, &models[0])));
}

#[test]
fn a_blocking_call_does_not_wait_behind_the_queue() {
    let folder = project_with_models("steal");
    for index in 0..load_workers() {
        write_triangle(&folder, &format!("assets/models/busy{index}.gltf"), 1.0);
    }
    let assets = Assets::new(&folder);
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let (reached, reaches) = mpsc::channel();
    let reached = Mutex::new(reached);
    assets.set_load_gate(move |path| {
        if path.contains("busy") {
            reached.lock().unwrap().send(()).unwrap();
            let _ = released.lock().unwrap().recv_timeout(LIMIT);
        }
    });
    // Every worker is busy, so the next file stays in the queue.
    let busy: Vec<_> = (0..load_workers())
        .map(|index| assets.request_model(&format!("assets/models/busy{index}.gltf")))
        .collect();
    for _ in 0..load_workers() {
        reaches.recv_timeout(LIMIT).expect("every worker started");
    }
    let queued = assets.request_model(A);
    assert_eq!(queued.status(), LoadStatus::Loading);

    // Read here and now: every worker is still held below.
    assert!(assets.model_blocking(A).is_some());
    assert_eq!(queued.status(), LoadStatus::Ready);

    drop(release);
    for ticket in busy {
        assert_eq!(ticket.wait(), LoadStatus::Ready);
    }
    assert_eq!(assets.progress(), (0, 0));
}

#[test]
fn a_file_forgotten_while_it_loads_is_read_again() {
    let folder = project_with_models("forget");
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, A);
    let ticket = assets.request_model(A);
    reached.recv_timeout(LIMIT).expect("the load started");

    write_triangle(&folder, A, 4.0);
    assets.forget(A);
    release.send(()).unwrap();
    // The second read goes through the gate too, so let that one by.
    reached.recv_timeout(LIMIT).expect("read again");
    release.send(()).unwrap();

    assert_eq!(ticket.wait(), LoadStatus::Ready);
    assert_eq!(assets.model_ready(A).unwrap().size[0], 4.0);
}

#[test]
fn a_model_handed_over_while_it_loads_wins() {
    let folder = project_with_models("insert");
    write_triangle(&folder, B, 4.0);
    let inserted = load_model(
        &fs::read(folder.join(B)).unwrap(),
        &folder.join("assets/models"),
        ModelFormat::Gltf,
    )
    .unwrap();
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, A);
    let ticket = assets.request_model(A);
    reached.recv_timeout(LIMIT).expect("the load started");

    let inserted = assets.insert(A, inserted);
    assert_eq!(ticket.status(), LoadStatus::Ready, "over at once");
    release.send(()).unwrap();
    assert!(Arc::ptr_eq(&assets.model_blocking(A).unwrap(), &inserted));
    assert_eq!(assets.progress(), (0, 0));
}

#[test]
fn a_reload_leaves_the_old_version_in_use_until_the_new_one_is_read() {
    let folder = project_with_models("reload");
    let assets = Assets::new(&folder);
    let old = assets.model_blocking(A).unwrap();
    write_triangle(&folder, A, 4.0);
    let (release, reached) = gate_hold(&assets, A);

    let reloading = assets.clone();
    let thread = std::thread::spawn(move || reloading.reload(A));
    reached.recv_timeout(LIMIT).expect("the reload started");
    assert!(
        Arc::ptr_eq(&assets.model_ready(A).unwrap(), &old),
        "the old version is there, and the lock is free"
    );
    release.send(()).unwrap();
    assert_eq!(thread.join().unwrap(), Reload::Reloaded);
    assert_eq!(assets.model_ready(A).unwrap().size[0], 4.0);
}

#[test]
fn a_decoder_that_panics_fails_its_file_and_wakes_those_waiting() {
    let folder = project_with_models("panic");
    let assets = Assets::new(&folder);
    assets.set_load_gate(|_| panic!("the decoder broke"));
    let ticket = assets.request_model(A);
    assert_eq!(ticket.wait(), LoadStatus::Failed);
    assert!(assets.model_blocking(A).is_none(), "remembered");
    let errors = assets.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("the decoder failed"), "{errors:?}");
}

#[test]
fn a_material_request_finishes_after_its_textures() {
    let folder = project("material");
    fs::write(folder.join("assets/textures/a.png"), model::checker_png()).unwrap();
    fs::write(folder.join("assets/textures/b.png"), model::checker_png()).unwrap();
    fs::write(
        folder.join("assets/materials/two.material.json"),
        r#"{ "maps": {
            "color": { "path": "assets/textures/a.png" },
            "normal": { "path": "assets/textures/b.png" }
        } }"#,
    )
    .unwrap();
    let assets = Assets::new(&folder);
    assets.load_textures_in_background(|| {});
    let (release, reached) = gate_hold(&assets, "assets/textures/a.png");

    let ticket = assets.request_material("assets/materials/two.material.json");
    reached.recv_timeout(LIMIT).expect("the texture started");
    assert_eq!(ticket.status(), LoadStatus::Loading, "held by a texture");
    release.send(()).unwrap();

    assert_eq!(ticket.wait(), LoadStatus::Ready);
    assert!(assets.texture("assets/textures/a.png").is_some());
    assert!(assets.texture("assets/textures/b.png").is_some());
    assert!(assets.texture_progress().is_none());
}

#[test]
fn a_material_with_a_missing_texture_fails_its_request() {
    let folder = project("material-gone");
    fs::write(
        folder.join("assets/materials/one.material.json"),
        r#"{ "maps": { "color": { "path": "assets/textures/gone.png" } } }"#,
    )
    .unwrap();
    let assets = Assets::new(&folder);
    let ticket = assets.request_material("assets/materials/one.material.json");
    assert_eq!(ticket.wait(), LoadStatus::Failed);
    let errors = assets.take_errors();
    assert!(errors.iter().any(|e| e.contains("gone.png")), "{errors:?}");
}

#[test]
fn a_request_goes_by_the_files_name() {
    for (path, kind) in [
        ("assets/models/a.glb", Kind::Model),
        ("assets/models/a.fbx#Arm", Kind::Model),
        ("assets/materials/a.material.json", Kind::MaterialAndMaps),
        ("assets/textures/a.PNG", Kind::Texture),
        ("assets/sky/noon.exr", Kind::SkyImage),
        ("assets/probes/a.probe.hdr", Kind::ReflectionProbe),
        ("assets/probes/a.lightprobes.json", Kind::LightProbes),
        ("assets/data/a.data.json", Kind::DataTable),
        ("assets/grading/warm.cube", Kind::ColorLookup),
        ("assets/audio/a.wav", Kind::File),
    ] {
        assert_eq!(Kind::of(path), kind, "{path}");
    }
}

#[test]
fn any_project_file_can_be_requested() {
    let folder = project("any");
    fs::create_dir_all(folder.join("assets/audio")).unwrap();
    fs::write(folder.join("assets/audio/a.wav"), b"sound").unwrap();
    write_triangle(&folder, A, 1.0);
    let assets = Assets::new(&folder);
    assert_eq!(
        assets.request("assets/audio/a.wav").wait(),
        LoadStatus::Ready
    );
    assert_eq!(assets.request(A).wait(), LoadStatus::Ready);
    assert!(assets.model_ready(A).is_some());

    assert_eq!(
        assets.request("assets/audio/b.wav").wait(),
        LoadStatus::Failed
    );
    assert_eq!(assets.request("../a.wav").wait(), LoadStatus::Failed);
    let errors: HashSet<String> = assets.take_errors().into_iter().collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn a_workers_progress_counts_everything_queued() {
    let folder = project_with_models("progress");
    let assets = Assets::new(&folder);
    let (release, reached) = gate_hold(&assets, A);
    let a = assets.request_model(A);
    reached.recv_timeout(LIMIT).expect("the load started");
    assert_eq!(assets.progress(), (0, 1));
    release.send(()).unwrap();
    a.wait();
    assert_eq!(
        assets.progress(),
        (0, 0),
        "nothing left: the count starts over"
    );
}

#[test]
fn a_request_for_a_textures_pixels_decodes_a_cooked_texture_ahead() {
    let folder = project("pixels");
    let path = "assets/textures/ground.png";
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let png = png.into_inner();
    fs::write(folder.join(path), &png).unwrap();
    let stamp = SourceStamp::of(&folder.join(path), &png);
    let cooked = CookedTexture {
        format: CookedFormat::Bc7,
        width: 4,
        height: 4,
        levels: vec![vec![0; 16]; 3],
        has_alpha: false,
        average: [10, 20, 30, 255],
    };
    write_cooked(&folder, path, stamp, &cooked).unwrap();
    let assets = Assets::new(&folder);
    assets.load_textures_in_background(|| {});

    // A plain request leaves a cooked texture's pixels in its file.
    assert_eq!(assets.request_texture(path).wait(), LoadStatus::Ready);
    let texture = assets.texture(path).unwrap();
    assert!(texture.cooked().is_some());
    assert!(!texture.has_pixels());

    // Asked for its pixels, the worker decodes them, so the first frame
    // that reads them (a terrain layer's) doesn't.
    assert_eq!(
        assets.request_texture_pixels(path).wait(),
        LoadStatus::Ready
    );
    assert!(texture.has_pixels());
    assert_eq!(&texture.rgba()[..4], &[10, 20, 30, 255]);
    let again = assets.request_texture_pixels(path);
    assert_eq!(again.status(), LoadStatus::Ready, "known at once");
    assert_eq!(
        assets
            .request_texture_pixels("assets/textures/gone.png")
            .wait(),
        LoadStatus::Failed
    );
    let _ = fs::remove_dir_all(folder);
}
