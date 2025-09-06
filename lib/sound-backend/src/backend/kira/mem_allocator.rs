use anyhow::anyhow;
use base::{
    hash::{Hash, generate_hash_for},
    linked_hash_map_view::FxLinkedHashMap,
};
use hiarc::{HiFnOnce, Hiarc, hi_closure, hiarc_safer_arc_mutex};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};

use sound::sound_mt_types::{SoundBackendMemory, SoundBackendMemoryCleaner};

#[hiarc_safer_arc_mutex]
#[derive(Debug, Hiarc, Default)]
struct MemoryAllocatorInner {
    id_gen: u128,

    flushed_sound_memory: FxLinkedHashMap<u128, StaticSoundData>,
    hashed_sound_memory: FxLinkedHashMap<Hash, (u64, StaticSoundData)>,
}

#[hiarc_safer_arc_mutex]
impl MemoryAllocatorInner {
    pub fn next_id(&mut self) -> u128 {
        let id = self.id_gen;
        self.id_gen += 1;
        id
    }

    pub fn add(&mut self, id: u128, data: StaticSoundData) {
        self.flushed_sound_memory.insert(id, data);
    }

    pub fn remove(&mut self, id: &u128) -> Option<StaticSoundData> {
        self.flushed_sound_memory.remove(id)
    }

    pub fn contains(&mut self, id: &u128) -> bool {
        self.flushed_sound_memory.contains_key(id)
    }

    pub fn add_hashed(&mut self, hash: Hash, data: StaticSoundData) {
        let entry = self
            .hashed_sound_memory
            .entry(hash)
            .or_insert_keep_order((0, data));
        entry.0 += 1;
    }

    pub fn remove_hashed(&mut self, hash: &Hash) {
        let counter = if let Some((counter, _)) = self.hashed_sound_memory.get_mut(hash) {
            *counter -= 1;
            *counter
        } else {
            0
        };
        if counter == 0 {
            self.hashed_sound_memory.remove(hash);
        }
    }

    #[allow(clippy::multiple_bound_locations)]
    pub fn get_or_add<F>(&mut self, hash: Hash, f: F) -> anyhow::Result<StaticSoundData>
    where
        F: HiFnOnce<(), Vec<u8>>,
    {
        let sound_data = if let Some(sound_data) = self
            .hashed_sound_memory
            .get(&hash)
            .cloned()
            .map(|(_, data)| data)
        {
            sound_data
        } else {
            StaticSoundData::from_cursor(std::io::Cursor::new(f.call_once(())))?
        };
        self.add_hashed(hash, sound_data.clone());
        Ok(sound_data)
    }
}

#[derive(Debug, Hiarc)]
pub struct HashedStaticSound {
    pub data: StaticSoundData,
    hash: Hash,
    allocator: MemoryAllocatorInner,
}

impl HashedStaticSound {
    fn new(hash: Hash, data: StaticSoundData, allocator: MemoryAllocatorInner) -> Self {
        allocator.add_hashed(hash, data.clone());
        Self {
            data,
            hash,
            allocator,
        }
    }
}

impl Drop for HashedStaticSound {
    fn drop(&mut self) {
        self.allocator.remove_hashed(&self.hash);
    }
}

/// allocates memory so the kira backend can interpret it
#[derive(Debug, Hiarc, Default, Clone)]
pub struct MemoryAllocator {
    inner: MemoryAllocatorInner,
}

impl MemoryAllocator {
    pub fn mem_alloc(&self, size: usize) -> SoundBackendMemory {
        assert!(size > 0, "an allocation of 0 is an implementation bug");
        let id = self.inner.next_id();

        #[derive(Debug)]
        struct Deallocator {
            inner: MemoryAllocatorInner,
        }

        impl SoundBackendMemoryCleaner for Deallocator {
            fn destroy(&self, id: u128, hash: Option<Hash>) {
                self.inner.remove(&id);
                if let Some(hash) = hash {
                    self.inner.remove_hashed(&hash);
                }
            }
        }

        SoundBackendMemory::FlushableVector {
            data: vec![0; size],
            id,
            deallocator: Some(Box::new(Deallocator {
                inner: self.inner.clone(),
            })),
            hash: None,
            err: None,
        }
    }

    pub fn try_flush_mem(&self, mem: &mut SoundBackendMemory) -> anyhow::Result<()> {
        match mem {
            SoundBackendMemory::FlushableVector {
                data,
                id,
                hash,
                err: sound_err,
                ..
            } => {
                if let Some(err) = sound_err.take() {
                    *sound_err = Some(anyhow!("{err}"));
                    return Err(err);
                }
                anyhow::ensure!(!data.is_empty(), "sound memory was already taken.");
                let data = std::mem::take(data);
                let snd_hash = generate_hash_for(&data);
                let sound_data = match self.inner.get_or_add(
                    snd_hash,
                    hi_closure!([data: Vec<u8>], |_: ()| -> Vec<u8> { data }),
                ) {
                    Ok(sound_data) => sound_data,
                    Err(err) => {
                        *sound_err = Some(anyhow!("{}", err));
                        return Err(err);
                    }
                };
                *hash = Some(snd_hash);
                self.inner.add(*id, sound_data);

                Ok(())
            }
            SoundBackendMemory::Vector { .. } => Err(anyhow!(
                "data is not marked as flushable,\
                that can happen if it got serialized/deserialized"
            )),
        }
    }

    pub fn sound_data_from_mem(
        &self,
        mut mem: SoundBackendMemory,
    ) -> anyhow::Result<HashedStaticSound> {
        match &mut mem {
            SoundBackendMemory::FlushableVector { id, .. } => {
                let id = *id;
                if !self.inner.contains(&id) {
                    self.try_flush_mem(&mut mem)?;
                }
                let SoundBackendMemory::FlushableVector { hash, .. } = mem else {
                    panic!("implementation bug.")
                };
                let hash = hash.ok_or_else(|| anyhow!("hash was not set, implementation bug"))?;
                self.inner
                    .remove(&id)
                    .map(|snd| HashedStaticSound::new(hash, snd, self.inner.clone()))
                    .ok_or(anyhow!("static sound data could not be created"))
            }
            SoundBackendMemory::Vector { data } => {
                let hash = generate_hash_for(data);
                let sound_data = self.inner.get_or_add(
                    hash,
                    hi_closure!([data: &mut Vec<u8>], |_: ()| -> Vec<u8> { std::mem::take(data) }),
                )?;
                let res = HashedStaticSound::new(hash, sound_data, self.inner.clone());
                // Remove the implicit reference again
                self.inner.remove_hashed(&hash);
                Ok(res)
            }
        }
    }

    pub(crate) fn fake_sound(&self) -> HashedStaticSound {
        HashedStaticSound::new(
            Default::default(),
            StaticSoundData {
                sample_rate: 4800,
                frames: [].into(),
                settings: StaticSoundSettings::new(),
                slice: None,
            },
            self.inner.clone(),
        )
    }
}
