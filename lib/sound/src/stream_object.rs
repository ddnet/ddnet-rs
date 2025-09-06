use std::{rc::Rc, sync::Arc};

use hiarc::{Hiarc, hiarc_safer_rc_refcell};

use crate::{
    commands::{
        SoundCommand, SoundCommandState, SoundCommandStream, SoundCommandStreamObject,
        StreamObjectStream,
    },
    scene_object_shared::SceneObjectInner,
    stream::StreamDecoder,
    types::{StreamPlayBaseProps, StreamPlayProps},
};

/// the sound object handles events related to the internal sound data
#[hiarc_safer_rc_refcell]
#[derive(Debug, Hiarc)]
pub struct StreamObject {
    id: u128,
    // must outlive the scene
    scene: Rc<SceneObjectInner>,
}

#[hiarc_safer_rc_refcell]
impl StreamObject {
    pub fn new<F: StreamDecoder>(
        id: u128,
        handle: Arc<F>,
        props: StreamPlayProps,
        scene: Rc<SceneObjectInner>,
    ) -> Self {
        scene
            .backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::StreamObject(
                SoundCommandStreamObject::Create {
                    id,
                    scene_id: scene.id,
                    stream: StreamObjectStream(handle.clone()),
                    props,
                },
            )));

        Self { id, scene }
    }

    /// Update the properties of the stream play handle
    pub fn update(&self, props: StreamPlayBaseProps) {
        self.scene
            .backend_handle
            .add_cmd(SoundCommand::Stream(SoundCommandStream::Update {
                stream_id: self.id,
                scene_id: self.scene.id,
                props,
            }));
    }

    /// Resumes the stream.
    /// if the handle is already resumed/playing, nothing happens.
    pub fn resume(&self) {
        self.scene
            .backend_handle
            .add_cmd(SoundCommand::Stream(SoundCommandStream::Resume {
                stream_id: self.id,
                scene_id: self.scene.id,
            }));
    }

    /// Pauses the stream.
    /// If the handle is already paused, nothing happens.
    pub fn pause(&self) {
        self.scene
            .backend_handle
            .add_cmd(SoundCommand::Stream(SoundCommandStream::Pause {
                stream_id: self.id,
                scene_id: self.scene.id,
            }));
    }
}

#[hiarc_safer_rc_refcell]
impl Drop for StreamObject {
    fn drop(&mut self) {
        self.scene
            .backend_handle
            .add_cmd(SoundCommand::State(SoundCommandState::StreamObject(
                SoundCommandStreamObject::Destroy {
                    id: self.id,
                    scene_id: self.scene.id,
                },
            )));
    }
}
