use std::fmt::Debug;

use anyhow::Error;
use hiarc::Hiarc;
use pool::mt_datatypes::PoolUnclearedVec;
use thiserror::Error;

pub type OffscreenCanvasId = u128;

#[derive(Debug, Clone, Copy)]
pub enum FetchCanvasIndex {
    Onscreen,
    Offscreen(OffscreenCanvasId),
}

#[derive(Debug, Hiarc, Error)]
pub enum FetchCanvasError {
    #[error("canvas with the id, which was obtained by `current_fetch_index`, was not found.")]
    CanvasNotFound,
    #[error("the backend had an error: {0}")]
    DriverErr(String),
}

impl From<Error> for FetchCanvasError {
    fn from(value: Error) -> Self {
        FetchCanvasError::DriverErr(value.to_string())
    }
}

#[derive(Debug, Hiarc)]
pub struct BackendPresentedImageDataRgba {
    pub width: u32,
    pub height: u32,
    pub dest_data_buffer: PoolUnclearedVec<u8>,
}

pub trait BackendFrameFetcher: Debug + Sync + Send + 'static {
    fn next_frame(&self, frame_data: BackendPresentedImageDataRgba);

    /// generally a frame fetcher should only fetch the content of a specific canvas
    /// if for whatever reason it changes it can however,
    /// the backend must respect it for every frame.
    fn current_fetch_index(&self) -> FetchCanvasIndex;

    /// informs that fetching failed for some reason
    fn fetch_err(&self, err: FetchCanvasError);
}
