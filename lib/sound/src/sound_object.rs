use std::rc::Rc;

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    commands::{SoundCommand, SoundCommandSoundObject, SoundCommandState},
    scene_object_shared::SceneObjectInner,
    sound_mt_types::SoundBackendMemory,
    sound_object_shared::SoundObjectInner,
    sound_play_handle::SoundPlayHandle,
    types::SoundPlayProps,
};

/// the sound object handles events related to the internal sound data
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct SoundObject {
    inner: Rc<SoundObjectInner>,

    id_gen: u128,
}

#[hiarc_safer_rc_refcell]
impl SoundObject {
    pub fn new(id: u128, mem: SoundBackendMemory, scene: Rc<SceneObjectInner>) -> Self {
        scene
            .backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::SoundObject(
                SoundCommandSoundObject::Create {
                    id,
                    scene_id: scene.id,
                    mem,
                },
            )));

        Self {
            inner: SoundObjectInner::new(id, scene),

            id_gen: 0,
        }
    }

    pub fn play(&mut self, props: SoundPlayProps) -> SoundPlayHandle {
        let id = self.id_gen;
        self.id_gen += 1;

        SoundPlayHandle::new(id, self.inner.clone(), props)
    }
}
