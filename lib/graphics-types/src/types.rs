use std::{fmt::Debug, num::NonZeroUsize};

use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use crate::commands::TexFlags;

#[derive(Debug, Hiarc, Clone, Copy, Serialize, Deserialize)]
pub enum GraphicsMemoryAllocationType {
    TextureRgbaU8 {
        width: NonZeroUsize,
        height: NonZeroUsize,
        flags: TexFlags,
    },
    TextureRgbaU82dArray {
        width: NonZeroUsize,
        height: NonZeroUsize,
        depth: NonZeroUsize,
        flags: TexFlags,
    },
    Buffer {
        required_size: NonZeroUsize,
    },
}

#[derive(Debug, Hiarc, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GraphicsMemoryAllocationMode {
    Immediate,
    Lazy,
}

#[derive(Debug)]
pub struct VideoMode {
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub window_width: i32,
    pub window_height: i32,
    pub refresh_rate: u32,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub format: u32,
}

impl VideoMode {
    pub const fn new(
        canvas_width: i32,
        canvas_height: i32,
        window_width: i32,
        window_height: i32,
        refresh_rate: u32,
        red: u32,
        green: u32,
        blue: u32,
        format: u32,
    ) -> VideoMode {
        VideoMode {
            canvas_width,
            canvas_height,
            window_width,
            window_height,
            refresh_rate,
            red,
            green,
            blue,
            format,
        }
    }
}

#[derive(Debug, Hiarc, Copy, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowProps {
    pub canvas_width: u32,
    pub canvas_height: u32,

    pub window_width: f64,
    pub window_height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DrawModes {
    Quads = 1,
    Lines = 2,
    Triangles = 3,
}

pub trait GraphicsBackendMemoryStaticCleaner: Debug + Send + Sync {
    fn destroy(&self, mem: &'static mut [u8]);
}

#[derive(Debug, Hiarc)]
pub struct GraphicsBackendMemoryStatic {
    pub mem: Option<&'static mut [u8]>,
    #[hiarc_skip_unsafe]
    pub deallocator: Option<Box<dyn GraphicsBackendMemoryStaticCleaner>>,
}

impl Drop for GraphicsBackendMemoryStatic {
    fn drop(&mut self) {
        if let Some(deallocator) = self.deallocator.take() {
            deallocator.destroy(self.mem.take().unwrap());
        }
    }
}

#[derive(Debug, Hiarc)]
pub enum GraphicsBackendMemoryAllocation {
    Static(GraphicsBackendMemoryStatic),
    Vector(Vec<u8>),
}

impl Serialize for GraphicsBackendMemoryAllocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Static { .. } => Err(serde::ser::Error::custom(
                "encoding static data is usually not what you want, because the allocation will be wasted.",
            )),
            Self::Vector(data) => serde::Serialize::serialize(data, serializer),
        }
    }
}

impl<'de> Deserialize<'de> for GraphicsBackendMemoryAllocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::Vector(<Vec<u8>>::deserialize(deserializer)?))
    }
}

#[derive(Debug, Hiarc)]
pub struct GraphicsBackendMemory {
    allocation: GraphicsBackendMemoryAllocation,
    ty: GraphicsMemoryAllocationType,
}

impl Serialize for GraphicsBackendMemory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (&self.allocation, self.ty).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GraphicsBackendMemory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (allocation, ty) = <(
            GraphicsBackendMemoryAllocation,
            GraphicsMemoryAllocationType,
        )>::deserialize(deserializer)?;

        let GraphicsBackendMemoryAllocation::Vector(mem) = allocation else {
            return Err(serde::de::Error::custom(
                "expected vector memory, found static instead.",
            ));
        };

        let required_len = match &ty {
            GraphicsMemoryAllocationType::TextureRgbaU8 { width, height, .. } => {
                width.get() * height.get() * 4
            }
            GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                width,
                height,
                depth,
                ..
            } => width.get() * height.get() * depth.get() * 4,
            GraphicsMemoryAllocationType::Buffer { required_size } => required_size.get(),
        };
        if required_len != mem.len() {
            return Err(serde::de::Error::custom(format!(
                "expected vector of size {} found {} instead.",
                required_len,
                mem.len(),
            )));
        }

        Ok(Self {
            allocation: GraphicsBackendMemoryAllocation::Vector(mem),
            ty,
        })
    }
}

impl GraphicsBackendMemory {
    pub fn new(
        allocation: GraphicsBackendMemoryAllocation,
        ty: GraphicsMemoryAllocationType,
    ) -> Self {
        Self { allocation, ty }
    }

    pub fn copy_from_slice(&mut self, slice: &[u8]) {
        match &mut self.allocation {
            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                mem, ..
            }) => mem.as_mut().unwrap().copy_from_slice(slice),
            GraphicsBackendMemoryAllocation::Vector(mem) => mem.copy_from_slice(slice),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.allocation {
            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                mem, ..
            }) => mem.as_ref().unwrap(),
            GraphicsBackendMemoryAllocation::Vector(mem) => mem.as_slice(),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.allocation {
            GraphicsBackendMemoryAllocation::Static(GraphicsBackendMemoryStatic {
                mem, ..
            }) => mem.as_mut().unwrap(),
            GraphicsBackendMemoryAllocation::Vector(mem) => mem.as_mut_slice(),
        }
    }

    pub fn take(
        self,
    ) -> (
        GraphicsBackendMemoryAllocation,
        GraphicsMemoryAllocationType,
    ) {
        (self.allocation, self.ty)
    }

    pub fn alloc_mut(&mut self) -> &mut GraphicsBackendMemoryAllocation {
        &mut self.allocation
    }

    pub fn usage(&self) -> &GraphicsMemoryAllocationType {
        &self.ty
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}
