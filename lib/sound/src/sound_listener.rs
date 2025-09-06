use std::rc::Rc;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};
use math::math::vector::vec2;

use crate::{
    commands::{SoundCommand, SoundCommandSoundListener, SoundCommandState},
    scene_object_shared::SceneObjectInner,
};

/// a sound listener is an object that listens on all sounds in a scene, it is allocated
/// by the sound listener handle ([`crate::sound_listener_handle::SoundListenerHandle`])
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundListener {
    id: u128,

    // must outlive the scene
    scene: Rc<SceneObjectInner>,

    forgotten: bool,
}

#[hiarc_safer_rc_refcell]
impl SoundListener {
    pub fn new(id: u128, pos: vec2, scene: Rc<SceneObjectInner>) -> Self {
        scene
            .backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::SoundListener(
                SoundCommandSoundListener::Create {
                    id,
                    scene_id: scene.id,
                    pos,
                },
            )));

        Self {
            id,
            scene,
            forgotten: false,
        }
    }

    pub fn update(&self, pos: vec2) {
        self.scene
            .backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::SoundListener(
                SoundCommandSoundListener::Update {
                    id: self.id,
                    scene_id: self.scene.id,
                    pos,
                },
            )));
    }
}

#[hiarc_safer_rc_refcell]
impl Drop for SoundListener {
    fn drop(&mut self) {
        if !self.forgotten {
            self.scene.backend_handle.add_cmd(SoundCommand::State(
                SoundCommandState::SoundListener(SoundCommandSoundListener::Destroy {
                    id: self.id,
                    scene_id: self.scene.id,
                }),
            ));
        }
    }
}
