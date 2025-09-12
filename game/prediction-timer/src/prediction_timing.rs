use std::{collections::VecDeque, num::NonZeroU64, ops::Deref, time::Duration};

#[derive(Debug, Clone, Copy)]
pub struct PredictionTimingCollection {
    pub max: Duration,
    pub min: Duration,
    pub average: Duration,
    pub avg_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PredictionStatsCollection {
    pub packets_sent: u64,
    pub packets_lost: u64,
}

#[derive(Debug, Clone)]
pub struct PredictionTiming {
    /// last seconds of highest/lowest/average pings per second
    pub last_secs_of_pings: VecDeque<PredictionTimingCollection>,
    pub smooth_max_ping: f64,
    /// last snap differences in timing
    pub last_snaps_average: (f64, usize),
    /// max of the last seconds of frame times
    pub frame_time_max: VecDeque<Duration>,
    /// only for debugging
    pub last_forced_weight: f64,

    pub cur_whole_second: u64,
    pub cur_duration_snap: Duration,
    pub cur_whole_second_frametime: u64,
    pub cur_whole_second_stats: u64,

    /// current smooth time that applied to the timer
    pub smooth_adjustment_time: f64,
    pub jitter_range: Duration,

    pub last_secs_of_packets_stats: VecDeque<PredictionStatsCollection>,
}

impl PredictionTiming {
    /// the more jitter we got, the more values we use from the past
    /// to have more stability in the values
    pub fn calc_farsight_of_jitter(&self) -> usize {
        // TODO: random values
        if self.jitter_range.as_millis() < 2 {
            2
        } else if self.jitter_range.as_millis() < 20 {
            5
        } else if self.jitter_range.as_millis() < 100 {
            20
        } else if self.jitter_range.as_millis() < 1000 {
            40
        } else {
            60
        }
    }

    /// the more jitter we got, the more values we use from the past
    /// This version is only for ping avg
    pub fn calc_farsight_of_jitter_avg(&self) -> usize {
        // TODO: random values
        if self.jitter_range.as_millis() < 2 {
            1
        } else if self.jitter_range.as_millis() < 20 {
            2
        } else if self.jitter_range.as_millis() < 100 {
            3
        } else if self.jitter_range.as_millis() < 1000 {
            4
        } else {
            10
        }
    }

    fn ping_max_impl(&self, count: usize) -> Duration {
        self.last_secs_of_pings
            .iter()
            .take(count)
            .max_by(|a1, a2| a1.max.cmp(&a2.max))
            .map(|v| v.max)
            .unwrap()
    }

    /// get's the highest value
    pub fn ping_max(&self) -> Duration {
        self.ping_max_impl(self.calc_farsight_of_jitter())
    }

    fn ping_min_impl(&self, count: usize) -> Duration {
        self.last_secs_of_pings
            .iter()
            .take(count)
            .min_by(|a1, a2| a1.min.cmp(&a2.min))
            .map(|v| v.min)
            .unwrap()
    }

    /// get's the lowest value
    pub fn ping_min(&self) -> Duration {
        self.ping_min_impl(self.calc_farsight_of_jitter())
    }

    /// get's the average value
    pub fn ping_average(&self) -> Duration {
        let count = self.calc_farsight_of_jitter_avg();
        Duration::from_nanos(
            (self
                .last_secs_of_pings
                .iter()
                .take(count)
                .map(|ping| ping.average.as_nanos() / ping.avg_count as u128)
                .sum::<u128>()
                / count as u128) as u64,
        )
    }
}

#[derive(Debug)]
pub struct PredictionTimer {
    timing: PredictionTiming,
}

impl PredictionTimer {
    /// small "extra" offset to smooth out some _normal_ jitter in frame times
    /// 500us: assumed frame time jitter
    pub const PREDICTION_MARGIN_FRAME_TIME: Duration = Duration::from_micros(500);
    /// ~500us jitter/overhead of the network implementation on the server and on the client combined.
    pub const PREDICTION_MARGIN_NETWORK: Duration = Duration::from_micros(500);

    pub fn new(first_ping: Duration, cur_time: Duration) -> Self {
        Self {
            timing: PredictionTiming {
                last_secs_of_pings: vec![PredictionTimingCollection {
                    max: first_ping,
                    min: first_ping,
                    average: first_ping,
                    avg_count: 1,
                }]
                .into(),
                smooth_max_ping: 0.0,
                last_snaps_average: (0.0, 0),
                last_forced_weight: 0.0,

                frame_time_max: vec![Duration::ZERO].into(),
                cur_whole_second: cur_time.as_secs(),
                cur_duration_snap: cur_time,
                cur_whole_second_frametime: cur_time.as_secs(),
                cur_whole_second_stats: cur_time.as_secs(),

                smooth_adjustment_time: 0.0,
                jitter_range: Duration::ZERO,

                last_secs_of_packets_stats: vec![PredictionStatsCollection {
                    packets_lost: 0,
                    packets_sent: 0,
                }]
                .into(),
            },
        }
    }

    /// Take a snapshot of the predicting timing.
    /// Useful for debugging
    pub fn snapshot(&self) -> PredictionTiming {
        self.timing.clone()
    }

    fn calc_snap_farsight_of_jitter(&self) -> Duration {
        // TODO: random values
        if self.timing.jitter_range.as_millis() < 2 {
            Duration::from_millis(1000)
        } else if self.timing.jitter_range.as_millis() < 20 {
            Duration::from_millis(3000)
        } else if self.timing.jitter_range.as_millis() < 100 {
            Duration::from_millis(5000)
        } else if self.timing.jitter_range.as_millis() < 1000 {
            Duration::from_millis(10000)
        } else {
            Duration::from_millis(20000)
        }
    }

    pub fn calc_jitter_range(&self) -> Duration {
        let count = if self.timing.jitter_range.as_millis() > 500 {
            10
        } else {
            3
        };
        self.ping_max_impl(count) - self.ping_min_impl(count)
    }

    pub fn add_ping(&mut self, ping: Duration, cur_time: Duration) {
        let whole_second = cur_time.as_secs().max(self.timing.cur_whole_second);
        // more than 3 seconds of laggs are not supported.
        let ping = ping.clamp(Duration::ZERO, Duration::from_secs(3));

        let old_max = self.ping_max();
        if whole_second > self.timing.cur_whole_second {
            self.timing.jitter_range = self.calc_jitter_range();

            let diff = whole_second - self.timing.cur_whole_second;

            let max_items = self.calc_farsight_of_jitter();
            // only last seconds are of interest
            self.timing
                .last_secs_of_pings
                .truncate((max_items as u64 - diff.min(max_items as u64)) as usize);
            self.timing
                .last_secs_of_pings
                .push_front(PredictionTimingCollection {
                    max: ping,
                    min: ping,
                    average: ping,
                    avg_count: 1,
                });
        } else {
            let cur_average = &mut self.timing.last_secs_of_pings[0];
            cur_average.max = cur_average.max.max(ping);
            cur_average.min = cur_average.min.min(ping);
            cur_average.avg_count += 1;
            cur_average.average =
                Duration::from_nanos((cur_average.average.as_nanos() + ping.as_nanos()) as u64);
        }
        let max = self.ping_max();
        self.timing.smooth_max_ping = (self.timing.smooth_max_ping + old_max.as_secs_f64()
            - max.as_secs_f64())
        .clamp(0.0, f64::MAX);
        self.timing.cur_whole_second = whole_second;
    }

    pub fn add_frametime(&mut self, time: Duration, cur_time: Duration) {
        let whole_second = cur_time
            .as_secs()
            .max(self.timing.cur_whole_second_frametime);
        // clamp to smth sane, anything beyond that is simply a lag outside of reach
        let time = time.clamp(Duration::ZERO, Duration::from_millis(1000 / 5));

        if whole_second > self.timing.cur_whole_second_frametime {
            let diff = whole_second - self.timing.cur_whole_second_frametime;
            let max_seconds = 2;
            // only last seconds are of interest
            self.timing
                .frame_time_max
                .truncate((max_seconds - diff.min(max_seconds)) as usize);
            self.timing.frame_time_max.push_front(time);
        } else {
            let cur_average = &mut self.timing.frame_time_max[0];
            *cur_average = (*cur_average).max(time);
        }
        self.timing.cur_whole_second_frametime = whole_second;
    }

    pub fn max_frametime(&self) -> Duration {
        *self.timing.frame_time_max.iter().max().unwrap()
    }

    pub fn packet_loss(&self) -> f64 {
        self.timing
            .last_secs_of_packets_stats
            .iter()
            .map(|s| s.packets_lost)
            .sum::<u64>() as f64
            / self
                .timing
                .last_secs_of_packets_stats
                .iter()
                .map(|s| s.packets_sent)
                .sum::<u64>()
                .max(1) as f64
    }

    fn extra_time_units_to_respect_by_packet_loss(&self) -> u32 {
        let loss = self.packet_loss();
        // hitting a 99% chance that an input won't be lost
        // > 75%, packets will be 100% lost
        if loss > 0.75 {
            // 20 is the maximum we want to support
            // such packet loss is unlikely anyway
            20
        }
        // > 50%
        else if loss > 0.5 {
            16
        }
        // > 10%
        else if loss > 0.1 {
            7
        }
        // > 0%
        else if loss > 0.0 {
            2
        } else {
            0
        }
    }

    /// `time_unit_time` is the time one tick takes in the physics
    pub fn extra_prediction_margin_by_packet_loss(&self, time_unit_time: Duration) -> Duration {
        time_unit_time * self.extra_time_units_to_respect_by_packet_loss()
    }

    /// How many time units to respect to have a high chance to
    /// not drop any inputs
    pub fn time_units_to_respect(&self, time_unit_time: Duration, max_units: NonZeroU64) -> u64 {
        (self.calc_jitter_range().as_nanos() / time_unit_time.as_nanos())
            .max(self.extra_time_units_to_respect_by_packet_loss() as u128)
            .clamp(1, max_units.get() as u128) as u64
    }

    /// Add current packet stats (packets sent & packets lost)
    /// to the prediction timer.
    /// This assumes the total number of packets lost & send, the timer
    /// internally then calculates the current packet loss for a time period,
    /// which the timer assumes as likely to influence the prediction.
    pub fn add_packet_stats(&mut self, cur_time: Duration, packets_sent: u64, packets_lost: u64) {
        let whole_second = cur_time.as_secs().max(self.timing.cur_whole_second_stats);

        if whole_second > self.timing.cur_whole_second_stats {
            let diff = whole_second - self.timing.cur_whole_second_stats;
            let max_seconds = 2;
            // only last seconds are of interest
            self.timing
                .last_secs_of_packets_stats
                .truncate((max_seconds - diff.min(max_seconds)) as usize);
            self.timing
                .last_secs_of_packets_stats
                .push_front(PredictionStatsCollection {
                    packets_sent,
                    packets_lost,
                });
        } else {
            let cur_stats = &mut self.timing.last_secs_of_packets_stats[0];
            cur_stats.packets_sent += packets_sent;
            cur_stats.packets_lost += packets_lost;
        }
        self.timing.cur_whole_second_stats = whole_second;
    }

    fn dist_ratio(x: f64, min: f64, max: f64, mid: f64) -> f64 {
        let relative_x = mid + x;
        let dist = if relative_x <= mid {
            let d = mid - min;
            if d < 0.00000001 {
                f64::MAX
            } else {
                (relative_x - mid).abs() / d
            }
        } else {
            let d = max - mid;
            if d < 0.00000001 {
                f64::MAX
            } else {
                (relative_x - mid).abs() / d
            }
        };

        // use expontential function
        let alpha = 2.0;
        let exp = (alpha * dist.clamp(0.0, 1.0) - alpha).exp();

        exp.clamp(0.0, 1.0) * x
    }

    /// prepares the smooth timer adjustment based on the likeliness
    /// of effect of the snapshot being off the expected time.
    pub fn add_snap(&mut self, snap_diff: f64, timestamp: Duration) {
        let ping_avg = self.ping_average().as_secs_f64() / 2.0
            + Self::PREDICTION_MARGIN_NETWORK.as_secs_f64() / 2.0;
        let ping_min =
            self.ping_min().as_secs_f64() / 2.0 + Self::PREDICTION_MARGIN_NETWORK.as_secs_f64();
        let ping_max = self.ping_max().as_secs_f64() / 2.0;

        let (last_snaps_average, weight) = self.timing.last_snaps_average;
        let mut adjust_factor = (snap_diff + last_snaps_average) / (weight + 1) as f64;

        // check how far off this snap diff is
        // if the snap is outside of the ping jitter, we have to assume a lag
        // either by network or bcs the client clock runs behind/fore
        let lag_weight = if adjust_factor > 0.0 && adjust_factor > (ping_max - ping_avg) {
            //dbg!((snap_diff * 1000.0, ping_max * 1000.0, ping_avg * 1000.0));
            //dbg!((snap_diff - (ping_max - ping_avg)) * 1000.0);
            Some(adjust_factor - (ping_max - ping_avg))
        } else if adjust_factor < 0.0 && adjust_factor.abs() > (ping_avg - ping_min) {
            //dbg!((snap_diff * 1000.0, ping_min * 1000.0, ping_avg * 1000.0));
            //dbg!(((ping_avg - ping_min) - snap_diff.abs()) * 1000.0);
            Some((ping_avg - ping_min) - adjust_factor.abs())
        } else {
            None
        };

        let cur_duration_snap = self.timing.cur_duration_snap;
        if (timestamp > cur_duration_snap + self.calc_snap_farsight_of_jitter())
            || lag_weight.is_some()
        {
            self.timing.cur_duration_snap = timestamp;
            if let Some(lag_weight) = lag_weight {
                /*dbg!(
                    lag_weight * 1000.0,
                    snap_diff * 1000.0,
                    ping_min * 1000.0,
                    ping_max * 1000.0,
                    ping_avg * 1000.0,
                );*/
                self.timing.last_forced_weight = lag_weight;
                adjust_factor = lag_weight;
                self.timing.last_snaps_average = (0.0, 0);
            } else {
                self.timing.last_snaps_average = (snap_diff, 1);
                adjust_factor =
                    Self::dist_ratio(adjust_factor, ping_min, ping_max, ping_avg) * adjust_factor;
            }
            /*let dist_ratio = Self::dist_ratio(
                adjust_factor,
                ping_min.as_secs_f64(),
                ping_max.as_secs_f64(),
                ping_avg.as_secs_f64(),
            );*/
            self.timing.smooth_adjustment_time = adjust_factor;
        } else {
            let (cur_average, avg_count) = &mut self.timing.last_snaps_average;
            *avg_count += 1;
            *cur_average += snap_diff;
        }
    }

    fn pred_max_smooth_impl(&mut self, time_unit_time: Duration, smooth_factor: f64) -> Duration {
        let max_ping = self.ping_max() + Self::PREDICTION_MARGIN_FRAME_TIME;
        let max_frame_time = self.max_frametime() + Self::PREDICTION_MARGIN_FRAME_TIME;
        let packet_loss_time = self.extra_prediction_margin_by_packet_loss(time_unit_time);

        // if the jitter is high, except some bigger jumps
        //dbg!(self.timing.smooth_max_ping * 1000.0, max.as_secs_f64() * 1000.0);
        self.timing.smooth_max_ping -=
            (self.timing.smooth_max_ping * 0.1).clamp(-0.01, 0.01) * smooth_factor;

        Duration::from_secs_f64(
            (max_ping.as_secs_f64()
                + max_frame_time.as_secs_f64()
                + self.timing.smooth_max_ping
                + packet_loss_time.as_secs_f64())
            .clamp(0.0, f64::MAX),
        )
    }

    /// Get's the time on which the average snap time
    /// should balance on.
    ///
    /// So basically the offset happens
    /// because of ping jitters.
    ///
    /// `time_unit_time` is the time one tick takes in the physics.
    pub fn pred_max_smoothing(&mut self, time_unit_time: Duration) -> Duration {
        self.pred_max_smooth_impl(time_unit_time, 1.0)
    }

    /// How many ticks the prediction is __at least__ ahead of the potential
    /// snapshots.
    ///
    /// Note: This is the minimum, so this function often returns more ticks
    /// than needed to make the calculation robust against very small prediction
    /// margins.
    pub fn pred_tick_offset(&mut self, time_unit_time: Duration) -> Duration {
        let pred_time = self.pred_max_smooth_impl(time_unit_time, 0.0);
        Duration::from_nanos(
            (pred_time.as_nanos()
                + (time_unit_time.as_nanos() - pred_time.as_nanos() % time_unit_time.as_nanos()))
                as u64,
        )
    }

    /// How much a single frame should adjust the prediction time
    pub fn smooth_adjustment_time(&mut self) -> f64 {
        let frame_time = self.max_frametime().as_secs_f64().clamp(0.000001, f64::MAX);

        let fps = 1.0 / frame_time;

        // 500 ms, basically ratio a lag compansates per call
        const HUNDERED_PERCENT_PER_S: f64 = 0.5;
        // basically the minimum fps we support
        const HUNDERED_PERCENT_PER_FPS: f64 = 50.0;
        let perc_adjust = self.timing.smooth_adjustment_time.abs() / HUNDERED_PERCENT_PER_S;
        let perc_fps_adjust = HUNDERED_PERCENT_PER_FPS / fps;

        let perc = (perc_adjust * perc_fps_adjust).clamp(0.0, 1.0);

        let adjust = self.timing.smooth_adjustment_time * perc;
        self.timing.smooth_adjustment_time *= 1.0 - perc;
        adjust
    }
}

impl Deref for PredictionTimer {
    type Target = PredictionTiming;
    fn deref(&self) -> &Self::Target {
        &self.timing
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use math::math::Rng;
    use textplots::{Chart, ColorPlot, Shape};

    use super::PredictionTimer;

    #[test]
    fn jitter_tests() {
        let ping_offset = 100.0 / 1000.0;
        let max_jitter = 600.0 / 1000.0;

        let mut cur_time = Duration::from_secs(1);
        let mut rng = Rng::new(0);
        let mut timer = PredictionTimer::new(
            Duration::from_secs_f64(ping_offset + max_jitter * 2.0),
            cur_time,
        );

        let mut time = cur_time - Duration::from_secs_f64(ping_offset / 2.0 + max_jitter);

        let mut chart_vals_time_diff = Vec::new();
        let mut chart_vals_max_ping = Vec::new();
        let mut chart_vals_avg_ping = Vec::new();
        let mut chart_vals_smooth_time = Vec::new();

        let snaps_per_sec = 20;
        let fps = 200;
        let snap_off = 2000;
        for i in 0..snap_off + 100 {
            cur_time = Duration::from_secs_f64(cur_time.as_secs_f64() + 1.0 / snaps_per_sec as f64);
            time = Duration::from_secs_f64(time.as_secs_f64() + 1.0 / snaps_per_sec as f64);

            let ratio = rng.random_float() as f64;
            let ratio_ping = rng.random_float() as f64 * 2.0;

            timer.add_ping(
                Duration::from_secs_f64(ping_offset + max_jitter * ratio_ping),
                cur_time,
            );

            let pred_max = timer
                .pred_max_smoothing(Duration::from_nanos(
                    (Duration::from_secs(1).as_nanos() / snaps_per_sec as u128) as u64,
                ))
                .as_secs_f64();

            let time_diff = cur_time.as_secs_f64() - time.as_secs_f64();
            let off = time_diff - (ping_offset / 2.0 + max_jitter * ratio) - pred_max;

            timer.add_snap(off, cur_time);

            let smooth_adjustment_time = timer.timing.smooth_adjustment_time;
            // simulate multiple frames (FPS) per predicition timer adjustment

            for _ in 0..(fps / snaps_per_sec) {
                timer.add_frametime(Duration::from_secs_f64(1.0 / fps as f64), cur_time);

                time = Duration::from_secs_f64(
                    (time.as_secs_f64() + timer.smooth_adjustment_time()).clamp(0.0, f64::MAX),
                );
            }

            dbg!(
                smooth_adjustment_time * 1000.0,
                off * 1000.0,
                time_diff * 1000.0,
                time,
                pred_max * 1000.0,
                timer.ping_max().as_secs_f64() * 1000.0,
                timer.ping_average().as_secs_f64() * 1000.0,
                max_jitter * ratio_ping,
                timer.timing.smooth_max_ping,
                timer.max_frametime().as_secs_f64() * 1000.0,
                timer.timing.jitter_range,
            );
            if i >= snap_off {
                chart_vals_time_diff.push(time_diff * 1000.0);
                chart_vals_max_ping.push(timer.ping_max().as_secs_f64() / 2.0 * 1000.0);
                chart_vals_avg_ping.push(timer.ping_average().as_secs_f64() * 1000.0);
                chart_vals_smooth_time.push(smooth_adjustment_time * 1000.0);
            }
        }
        let mut chart = Chart::new(
            chart_vals_time_diff.len() as u32 * 2,
            600,
            0.0,
            chart_vals_time_diff.len() as f32,
        );
        let shape = Shape::Continuous(Box::new(|x| {
            chart_vals_time_diff[x
                .round()
                .clamp(0.0, chart_vals_time_diff.len() as f32 - 1.0)
                as usize] as f32
        }));
        let shape_ping = Shape::Continuous(Box::new(|x| {
            chart_vals_max_ping
                [x.round().clamp(0.0, chart_vals_max_ping.len() as f32 - 1.0) as usize]
                as f32
        }));
        let shape_avg_ping = Shape::Continuous(Box::new(|x| {
            chart_vals_avg_ping
                [x.round().clamp(0.0, chart_vals_avg_ping.len() as f32 - 1.0) as usize]
                as f32
        }));
        let shape_smooth_time = Shape::Continuous(Box::new(|x| {
            chart_vals_smooth_time[x
                .round()
                .clamp(0.0, chart_vals_smooth_time.len() as f32 - 1.0)
                as usize] as f32
        }));
        let show_time_diff = true;
        let show_time_ping_half = true;
        let show_time_avg_ping = true;
        let show_time_smooth_time = true;
        let mut chart = &mut chart;

        if show_time_diff {
            chart = chart.linecolorplot(&shape, rgb::RGB8::new(255, 0, 0));
        }
        if show_time_ping_half {
            chart = chart.linecolorplot(&shape_ping, rgb::RGB8::new(0, 255, 0));
        }
        if show_time_avg_ping {
            chart = chart.linecolorplot(&shape_avg_ping, rgb::RGB8::new(0, 0, 255));
        }
        if show_time_smooth_time {
            chart = chart.linecolorplot(&shape_smooth_time, rgb::RGB8::new(255, 0, 255));
        }
        chart.y_axis();
        chart.x_axis();
        chart.display();
    }

    fn jitter_tests_test(
        rtt_offset: f64,
        half_rtt_jitter_range: f64,
        snap_count: usize,
        snaps_per_sec: i32,
        fps: i32,
        mut on_snap: impl FnMut(usize, f64, f64, f64, f64, f64),
        rng_seed: u64,
    ) -> PredictionTimer {
        // rng for ping jitter
        let mut rng = Rng::new(rng_seed);

        let mut cur_time = Duration::from_secs(100);

        // init timer with avg ping
        let mut timer = PredictionTimer::new(
            Duration::from_secs_f64(rtt_offset + half_rtt_jitter_range),
            cur_time,
        );
        // also add max & min ping
        timer.add_ping(
            Duration::from_secs_f64(rtt_offset + half_rtt_jitter_range * 2.0),
            cur_time,
        );
        timer.add_ping(Duration::from_secs_f64(rtt_offset), cur_time);

        let mut time = cur_time
            - timer.pred_max_smoothing(Duration::from_nanos(
                (Duration::from_secs(1).as_nanos() / snaps_per_sec as u128) as u64,
            ));

        let mut snaps = vec![(Duration::ZERO, 0); snap_count];
        let mut snap_time = cur_time;
        for (i, snap) in snaps.iter_mut().enumerate() {
            let ratio = rng.random_float() as f64;
            let server_packet_latency = rtt_offset / 2.0 + half_rtt_jitter_range * ratio;

            *snap = (
                Duration::from_secs_f64(snap_time.as_secs_f64() + server_packet_latency),
                i,
            );
            snap_time =
                Duration::from_secs_f64(snap_time.as_secs_f64() + 1.0 / snaps_per_sec as f64);
        }
        snaps.sort_by(|(time1, _), (time2, _)| time1.cmp(time2));

        for (i, snap) in snaps.iter_mut().enumerate() {
            let ratio_ping = rng.random_float() as f64 * 2.0;

            timer.add_ping(
                Duration::from_secs_f64(rtt_offset + half_rtt_jitter_range * ratio_ping),
                cur_time,
            );

            let pred_max = timer
                .pred_max_smoothing(Duration::from_nanos(
                    (Duration::from_secs(1).as_nanos() / snaps_per_sec as u128) as u64,
                ))
                .as_secs_f64();
            // one way from server to client (so only half rtt)

            let time_snap = snap.0;
            let time_client = time_snap.as_secs_f64() - time.as_secs_f64();
            let time_server = snap.1 as f64 * 1.0 / snaps_per_sec as f64;
            let time_diff = (time_server + pred_max) - time_client;
            let off = time_diff;

            timer.add_snap(off, cur_time);

            let smooth_adjustment_time = timer.timing.smooth_adjustment_time;

            // simulate multiple frames (FPS) per predicition timer adjustment
            for _ in 0..(fps / snaps_per_sec) {
                timer.add_frametime(Duration::from_secs_f64(1.0 / fps as f64), cur_time);

                time = Duration::from_secs_f64(
                    (time.as_secs_f64() - timer.smooth_adjustment_time()).clamp(0.0, f64::MAX),
                );
            }
            cur_time = Duration::from_secs_f64(cur_time.as_secs_f64() + 1.0 / snaps_per_sec as f64);

            on_snap(
                i,
                time_diff * 1000.0,
                timer.ping_min().as_secs_f64() * 1000.0,
                timer.ping_max().as_secs_f64() / 2.0 * 1000.0,
                timer.ping_average().as_secs_f64() * 1000.0,
                smooth_adjustment_time * 1000.0,
            );
        }
        timer
    }

    #[test]
    fn many_jitter_tests() {
        let tester = |latency_off: f64, latency_jitter_half: f64| {
            fn filter_vals(vals: Vec<f64>) -> Vec<f64> {
                vals.into_iter()
                    .enumerate()
                    .filter(|(snap_index, _)| snap_index % (3600 / 160) == 0)
                    .map(|(_, val)| val)
                    .collect()
            }
            let mut chart_vals_time_diff = Vec::new();
            let mut chart_vals_min_ping = Vec::new();
            let mut chart_vals_max_ping = Vec::new();
            let mut chart_vals_avg_ping = Vec::new();
            let mut chart_vals_smooth_time = Vec::new();
            fn draw_chart(
                chart_vals_time_diff: Vec<f64>,
                chart_vals_min_ping: Vec<f64>,
                chart_vals_max_ping: Vec<f64>,
                chart_vals_avg_ping: Vec<f64>,
                chart_vals_smooth_time: Vec<f64>,
            ) {
                let single_chart = |values: Vec<f64>, color: rgb::RGB8| {
                    let avg = values.iter().sum::<f64>() / values.len() as f64;
                    let max = values.iter().max_by(|&a1, &a2| a1.total_cmp(a2)).cloned();
                    let min = values.iter().min_by(|&a1, &a2| a1.total_cmp(a2)).cloned();

                    let values = filter_vals(values);

                    let mut chart =
                        Chart::new(values.len() as u32 * 2, 300, 0.0, values.len() as f32);
                    let shape = Shape::Continuous(Box::new(|x| {
                        values[x.round().clamp(0.0, values.len() as f32 - 1.0) as usize] as f32
                    }));
                    let mut chart = &mut chart;
                    chart = chart.linecolorplot(&shape, color);
                    chart.y_axis();
                    chart.x_axis();
                    chart.display();
                    println!("avg: {avg}");
                    println!("max: {:?}", max);
                    println!("min {:?}", min);
                };
                let show_time_diff = true;
                let show_time_min_ping = true;
                let show_time_max_ping = true;
                let show_time_avg_ping = true;
                let show_time_smooth_time = true;

                if show_time_diff {
                    single_chart(chart_vals_time_diff, rgb::RGB8::new(255, 0, 0));
                }
                if show_time_min_ping {
                    single_chart(chart_vals_min_ping, rgb::RGB8::new(0, 200, 0));
                }
                if show_time_max_ping {
                    single_chart(chart_vals_max_ping, rgb::RGB8::new(0, 255, 0));
                }
                if show_time_avg_ping {
                    single_chart(chart_vals_avg_ping, rgb::RGB8::new(0, 0, 255));
                }
                if show_time_smooth_time {
                    single_chart(chart_vals_smooth_time, rgb::RGB8::new(255, 0, 255));
                }
            }

            jitter_tests_test(
                latency_off / 1000.0,
                latency_jitter_half / 1000.0,
                3600,
                20,
                200,
                |_, time_diff, min_ping, max_ping, avg_ping, smooth_time| {
                    chart_vals_time_diff.push(time_diff);
                    chart_vals_min_ping.push(min_ping);
                    chart_vals_max_ping.push(max_ping);
                    chart_vals_avg_ping.push(avg_ping);
                    chart_vals_smooth_time.push(smooth_time);
                },
                10,
            );
            draw_chart(
                chart_vals_time_diff,
                chart_vals_min_ping,
                chart_vals_max_ping,
                chart_vals_avg_ping,
                chart_vals_smooth_time,
            );
        };
        tester(0.0, 0.0);
        tester(0.0, 100.0);
        tester(0.0, 600.0);
    }
}
