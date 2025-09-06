use graphics::handles::texture::texture::{TextureContainer, TextureContainer2dArray};
use map::skeleton::{
    MapSkeleton,
    animations::{
        AnimationsSkeleton, ColorAnimationSkeleton, PosAnimationSkeleton, SoundAnimationSkeleton,
    },
    config::ConfigSkeleton,
    groups::{
        MapGroupPhysicsSkeleton, MapGroupSkeleton, MapGroupsSkeleton,
        layers::{
            design::{
                MapLayerArbitrarySkeleton, MapLayerQuadSkeleton, MapLayerSkeleton,
                MapLayerSoundSkeleton, MapLayerTileSkeleton,
            },
            physics::MapLayerPhysicsSkeleton,
        },
    },
    metadata::MetadataSkeleton,
    resources::{MapResourceRefSkeleton, MapResourcesSkeleton},
};
use sound::{scene_object::SceneObject, sound_listener::SoundListener, sound_object::SoundObject};

use super::map_buffered::{
    PhysicsTileLayerVisuals, QuadLayerVisuals, SoundLayerSounds, TileLayerVisuals,
};

pub type MapVisualImage = MapResourceRefSkeleton<TextureContainer>;
pub type MapVisualImage2dArray = MapResourceRefSkeleton<TextureContainer2dArray>;
pub type MapVisualSound = MapResourceRefSkeleton<SoundObject>;

pub type MapVisualResources =
    MapResourcesSkeleton<(), TextureContainer, TextureContainer2dArray, SoundObject>;
pub type MapVisualGroups = MapGroupsSkeleton<
    (),
    (),
    PhysicsTileLayerVisuals,
    (),
    TileLayerVisuals,
    QuadLayerVisuals,
    SoundLayerSounds,
    (),
>;
pub type MapVisualPhysicsGroup = MapGroupPhysicsSkeleton<(), PhysicsTileLayerVisuals>;
pub type MapVisualPhysicsLayer = MapLayerPhysicsSkeleton<PhysicsTileLayerVisuals>;
pub type MapVisualGroup =
    MapGroupSkeleton<(), TileLayerVisuals, QuadLayerVisuals, SoundLayerSounds, ()>;
pub type MapVisualLayerArbitrary = MapLayerArbitrarySkeleton<()>;
pub type MapVisualLayerTile = MapLayerTileSkeleton<TileLayerVisuals>;
pub type MapVisualLayerQuad = MapLayerQuadSkeleton<QuadLayerVisuals>;
pub type MapVisualLayerSound = MapLayerSoundSkeleton<SoundLayerSounds>;
pub type MapVisualLayerBase<T, Q, S, A> = MapLayerSkeleton<T, Q, S, A>;
pub type MapVisualLayer =
    MapLayerSkeleton<TileLayerVisuals, QuadLayerVisuals, SoundLayerSounds, ()>;
pub type MapVisualAnimations = AnimationsSkeleton<(), ()>;
pub type MapVisualPosAnimation = PosAnimationSkeleton<()>;
pub type MapVisualColorAnimation = ColorAnimationSkeleton<()>;
pub type MapVisualSoundAnimation = SoundAnimationSkeleton<()>;

pub type MapVisualConfig = ConfigSkeleton<()>;
pub type MapVisualMetadata = MetadataSkeleton<()>;

#[derive(Debug)]
pub struct MapVisualProps {
    pub sound_scene: SceneObject,
    pub global_listener: SoundListener,
}

pub type MapVisual = MapSkeleton<
    MapVisualProps,
    (),
    TextureContainer,
    TextureContainer2dArray,
    SoundObject,
    (),
    (),
    PhysicsTileLayerVisuals,
    (),
    TileLayerVisuals,
    QuadLayerVisuals,
    SoundLayerSounds,
    (),
    (),
    (),
    (),
    (),
>;
