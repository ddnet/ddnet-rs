use std::borrow::{Borrow, BorrowMut};

use hiarc::Hiarc;
use serde::de::DeserializeOwned;

use crate::map::animations::{
    AnimBase, AnimPoint, AnimPointColor, AnimPointPos, AnimPointSound, Animations,
};

#[derive(Debug, Hiarc)]
pub struct AnimPointSkeleton<AP, T, const CHANNELS: usize> {
    pub def: AnimPoint<T, CHANNELS>,
    pub user: AP,
}

impl<AP, T, const CHANNELS: usize> From<AnimPointSkeleton<AP, T, CHANNELS>>
    for AnimPoint<T, CHANNELS>
{
    fn from(value: AnimPointSkeleton<AP, T, CHANNELS>) -> Self {
        value.def
    }
}

#[derive(Debug, Hiarc, Clone)]
pub struct AnimBaseSkeleton<A, AP: DeserializeOwned + PartialOrd + Clone> {
    pub def: AnimBase<AP>,

    pub user: A,
}

impl<A, AP: DeserializeOwned + PartialOrd + Clone> Borrow<AnimBase<AP>>
    for AnimBaseSkeleton<A, AP>
{
    fn borrow(&self) -> &AnimBase<AP> {
        &self.def
    }
}

impl<A, AP: DeserializeOwned + PartialOrd + Clone> BorrowMut<AnimBase<AP>>
    for AnimBaseSkeleton<A, AP>
{
    fn borrow_mut(&mut self) -> &mut AnimBase<AP> {
        &mut self.def
    }
}

pub type PosAnimationSkeleton<A> = AnimBaseSkeleton<A, AnimPointPos>;
pub type ColorAnimationSkeleton<A> = AnimBaseSkeleton<A, AnimPointColor>;
pub type SoundAnimationSkeleton<A> = AnimBaseSkeleton<A, AnimPointSound>;

impl<A> From<PosAnimationSkeleton<A>> for AnimBase<AnimPointPos> {
    fn from(value: PosAnimationSkeleton<A>) -> Self {
        Self {
            points: value.def.points,
            synchronized: value.def.synchronized,
            name: value.def.name,
        }
    }
}

impl<A> From<ColorAnimationSkeleton<A>> for AnimBase<AnimPointColor> {
    fn from(value: ColorAnimationSkeleton<A>) -> Self {
        Self {
            points: value.def.points,
            synchronized: value.def.synchronized,
            name: value.def.name,
        }
    }
}

impl<A> From<SoundAnimationSkeleton<A>> for AnimBase<AnimPointSound> {
    fn from(value: SoundAnimationSkeleton<A>) -> Self {
        Self {
            points: value.def.points,
            synchronized: value.def.synchronized,
            name: value.def.name,
        }
    }
}

#[derive(Debug, Hiarc, Default, Clone)]
pub struct AnimationsSkeleton<AS, A> {
    pub pos: Vec<PosAnimationSkeleton<A>>,
    pub color: Vec<ColorAnimationSkeleton<A>>,
    pub sound: Vec<SoundAnimationSkeleton<A>>,

    pub user: AS,
}

impl<AS, A> From<AnimationsSkeleton<AS, A>> for Animations {
    fn from(value: AnimationsSkeleton<AS, A>) -> Self {
        Self {
            pos: value.pos.into_iter().map(|i| i.into()).collect(),
            color: value.color.into_iter().map(|i| i.into()).collect(),
            sound: value.sound.into_iter().map(|i| i.into()).collect(),
        }
    }
}
