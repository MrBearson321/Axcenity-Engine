//! Uploading to the GPU on a thread of its own (Phase 80, world streaming).
//!
//! Meshes and color textures that come back into range would otherwise be
//! uploaded in the frame that first draws them, and a stall of a few
//! milliseconds for each is what shows as a hitch while moving through a
//! big world. While streaming is on (see
//! [`Renderer::set_background_uploads`](super::Renderer::set_background_uploads)),
//! they're uploaded here instead and appear a few frames later: a mesh
//! isn't drawn until it's ready, and a texture is white until it is.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::draw_list::{MeshSource, TextureSource};

use super::gpu::{upload_custom_mesh, upload_texture};
use super::skinning::Skinning;
use super::{GpuBytes, GpuMesh, GpuTexture};

enum Job {
    Mesh(Arc<dyn MeshSource>),
    Texture {
        source: Arc<dyn TextureSource>,
        sizing: (u32, u32),
    },
}

enum Done {
    Mesh(u64, Option<GpuMesh>),
    Texture(u64, Option<GpuTexture>),
}

/// What finished uploading: meshes and textures by key.
pub(super) type Finished = (Vec<(u64, Option<GpuMesh>)>, Vec<(u64, Option<GpuTexture>)>);

/// The thread, and what's been asked of it.
pub(super) struct Uploader {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    /// Keys asked for and not ready yet.
    meshes: HashSet<u64>,
    textures: HashSet<u64>,
}

impl Uploader {
    pub(super) fn start(
        device: wgpu::Device,
        queue: wgpu::Queue,
        (mesh_bytes, texture_bytes): (GpuBytes, GpuBytes),
        skinning: Skinning,
    ) -> Uploader {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, done) = mpsc::channel();
        // It stops when the pipeline is dropped: `recv` fails.
        std::thread::spawn(move || {
            while let Ok(job) = inbox.recv() {
                let result = match job {
                    Job::Mesh(source) => Done::Mesh(
                        source.key(),
                        upload_custom_mesh(&device, &mesh_bytes, &skinning, source.as_ref()),
                    ),
                    Job::Texture { source, sizing } => Done::Texture(
                        source.key(),
                        upload_texture(&device, &queue, &texture_bytes, source.as_ref(), sizing, 0),
                    ),
                };
                if outbox.send(result).is_err() {
                    return;
                }
            }
        });
        Uploader {
            jobs,
            done,
            meshes: HashSet::new(),
            textures: HashSet::new(),
        }
    }

    /// Asks for `source` once.
    pub(super) fn mesh(&mut self, source: &Arc<dyn MeshSource>) {
        if self.meshes.insert(source.key()) {
            let _ = self.jobs.send(Job::Mesh(source.clone()));
        }
    }

    /// Asks for `source` once, sized by `sizing`.
    pub(super) fn texture(&mut self, source: &Arc<dyn TextureSource>, sizing: (u32, u32)) {
        if self.textures.insert(source.key()) {
            let _ = self.jobs.send(Job::Texture {
                source: source.clone(),
                sizing,
            });
        }
    }

    /// What's finished since the last call.
    pub(super) fn take_finished(&mut self) -> Finished {
        let (mut meshes, mut textures) = (Vec::new(), Vec::new());
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Mesh(key, gpu) => {
                    self.meshes.remove(&key);
                    meshes.push((key, gpu));
                }
                Done::Texture(key, gpu) => {
                    self.textures.remove(&key);
                    textures.push((key, gpu));
                }
            }
        }
        (meshes, textures)
    }

    /// How many uploads are still on their way.
    pub(super) fn pending(&self) -> usize {
        self.meshes.len() + self.textures.len()
    }
}
