use std::{
    collections::{HashMap, HashSet},
    ops::RangeInclusive,
    time::Duration,
};

use egui::Color32;
use map::map::animations::{
    AnimBezier, AnimBeziers, AnimPoint, AnimPointColor, AnimPointCurveType, AnimPointPos,
    AnimPointSound, TimeDuration,
};
use math::math::vector::{ffixed, nffixed, vec1_base, vec3_base, vec4_base};

/// a channel of a graph [`Point`].
/// This could for example be the R channel in a RGBA point
pub trait PointChannel {
    fn value(&self) -> f32;
    fn set_value(&mut self, val: f32);
}

impl PointChannel for ffixed {
    fn value(&self) -> f32 {
        self.to_num()
    }

    fn set_value(&mut self, val: f32) {
        *self = ffixed::from_num(val);
    }
}

impl PointChannel for nffixed {
    fn value(&self) -> f32 {
        self.to_num()
    }

    fn set_value(&mut self, val: f32) {
        *self = nffixed::from_num(val.clamp(0.0, 1.0));
    }
}

/// Curve type of the point to the next point
#[derive(Debug)]
pub enum PointCurve {
    Step,
    Linear,
    Slow,
    Fast,
    Smooth,
    Bezier(Vec<AnimBezier>),
}

impl<T, const CHANNELS: usize> From<AnimPoint<T, CHANNELS>> for PointCurve {
    fn from(value: AnimPoint<T, CHANNELS>) -> Self {
        match value.curve_type {
            AnimPointCurveType::Step => PointCurve::Step,
            AnimPointCurveType::Linear => PointCurve::Linear,
            AnimPointCurveType::Slow => PointCurve::Slow,
            AnimPointCurveType::Fast => PointCurve::Fast,
            AnimPointCurveType::Smooth => PointCurve::Smooth,
            AnimPointCurveType::Bezier(bezier) => Self::Bezier({
                let mut res = vec![];

                for i in 0..CHANNELS {
                    res.push(bezier.value[i]);
                }

                res
            }),
        }
    }
}

impl<const CHANNELS: usize> From<PointCurve> for AnimPointCurveType<CHANNELS> {
    fn from(value: PointCurve) -> Self {
        match value {
            PointCurve::Step => Self::Step,
            PointCurve::Linear => Self::Linear,
            PointCurve::Slow => Self::Slow,
            PointCurve::Fast => Self::Fast,
            PointCurve::Smooth => Self::Smooth,
            PointCurve::Bezier(bezier) => Self::Bezier(AnimBeziers {
                value: bezier.try_into().unwrap(),
            }),
        }
    }
}

/// information about points in the graph
pub trait Point {
    /// time axis value of the point
    fn time_mut(&mut self) -> &mut Duration;
    fn time(&self) -> &Duration;
    /// e.g. for a color value this would be R, G, B(, A)
    /// (name, color, range of possible/allowed values, interface to interact with channel)
    fn channels(&mut self) -> Vec<(&str, Color32, RangeInclusive<f32>, &mut dyn PointChannel)>;

    fn channel_value_at(&self, channel_index: usize, other: &mut dyn Point, time: &Duration)
        -> f32;

    fn curve(&self) -> PointCurve;
    fn set_curve(&mut self, curve: PointCurve);
}

impl Point for AnimPointPos {
    fn time_mut(&mut self) -> &mut Duration {
        &mut self.time
    }
    fn time(&self) -> &Duration {
        &self.time
    }

    fn channels(&mut self) -> Vec<(&str, Color32, RangeInclusive<f32>, &mut dyn PointChannel)> {
        vec![
            ("x", Color32::YELLOW, f32::MIN..=f32::MAX, &mut self.value.x),
            ("y", Color32::KHAKI, f32::MIN..=f32::MAX, &mut self.value.y),
            ("r", Color32::BROWN, f32::MIN..=f32::MAX, &mut self.value.z),
        ]
    }

    fn channel_value_at(
        &self,
        channel_index: usize,
        other: &mut dyn Point,
        time: &Duration,
    ) -> f32 {
        let other_values = other.channels();
        let other_value = vec3_base::new(
            ffixed::from_num(other_values[0].3.value()),
            ffixed::from_num(other_values[1].3.value()),
            ffixed::from_num(other_values[2].3.value()),
        );
        AnimPoint::eval_curve_for(
            self,
            other.time(),
            &other_value,
            TimeDuration::new(time.as_secs() as i64, time.subsec_nanos() as i32),
        )[channel_index]
            .to_num()
    }

    fn curve(&self) -> PointCurve {
        self.clone().into()
    }

    fn set_curve(&mut self, curve: PointCurve) {
        self.curve_type = curve.into()
    }
}

impl Point for AnimPointColor {
    fn time_mut(&mut self) -> &mut Duration {
        &mut self.time
    }
    fn time(&self) -> &Duration {
        &self.time
    }

    fn channels(&mut self) -> Vec<(&str, Color32, RangeInclusive<f32>, &mut dyn PointChannel)> {
        vec![
            ("r", Color32::RED, 0.0..=1.0, &mut self.value.x),
            ("g", Color32::GREEN, 0.0..=1.0, &mut self.value.y),
            ("b", Color32::BLUE, 0.0..=1.0, &mut self.value.z),
            ("a", Color32::GRAY, 0.0..=1.0, &mut self.value.w),
        ]
    }

    fn channel_value_at(
        &self,
        channel_index: usize,
        other: &mut dyn Point,
        time: &Duration,
    ) -> f32 {
        let other_values = other.channels();
        let other_value = vec4_base::new(
            nffixed::from_num(other_values[0].3.value()),
            nffixed::from_num(other_values[1].3.value()),
            nffixed::from_num(other_values[2].3.value()),
            nffixed::from_num(other_values[3].3.value()),
        );
        AnimPoint::eval_curve_for(
            self,
            other.time(),
            &other_value,
            TimeDuration::new(time.as_secs() as i64, time.subsec_nanos() as i32),
        )[channel_index]
            .to_num()
    }

    fn curve(&self) -> PointCurve {
        self.clone().into()
    }

    fn set_curve(&mut self, curve: PointCurve) {
        self.curve_type = curve.into()
    }
}

impl Point for AnimPointSound {
    fn time_mut(&mut self) -> &mut Duration {
        &mut self.time
    }
    fn time(&self) -> &Duration {
        &self.time
    }

    fn channels(&mut self) -> Vec<(&str, Color32, RangeInclusive<f32>, &mut dyn PointChannel)> {
        vec![("v", Color32::GOLD, 0.0..=1.0, &mut self.value.x)]
    }

    fn channel_value_at(
        &self,
        channel_index: usize,
        other: &mut dyn Point,
        time: &Duration,
    ) -> f32 {
        let other_values = other.channels();
        let other_value = vec1_base::new(nffixed::from_num(other_values[0].3.value()));
        AnimPoint::eval_curve_for(
            self,
            other.time(),
            &other_value,
            TimeDuration::new(time.as_secs() as i64, time.subsec_nanos() as i32),
        )[channel_index]
            .to_num()
    }

    fn curve(&self) -> PointCurve {
        self.clone().into()
    }

    fn set_curve(&mut self, curve: PointCurve) {
        self.curve_type = curve.into()
    }
}

/// a group of points
pub struct PointGroup<'a> {
    /// the name of the point collection (e.g. "Color" or "Position")
    pub name: &'a str,
    /// an opaque type that implements [`Point`]
    pub points: Vec<&'a mut dyn Point>,
    /// timeline graph - currently selected points (e.g. by a pointer click)
    pub selected_points: &'a mut HashSet<usize>,
    /// timeline graph - currently hovered point (e.g. by a pointer)
    pub hovered_point: &'a mut Option<usize>,
    /// value graph - currently selected points + their channels (e.g. by a pointer click)
    pub selected_point_channels: &'a mut HashMap<usize, HashSet<usize>>,
    /// value graph - currently hovered points & their channel (e.g. by a pointer)
    pub hovered_point_channel: &'a mut HashMap<usize, HashSet<usize>>,
}
