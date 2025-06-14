use std::time::Duration;

/// Duration to strings for race timers.
pub trait DurationToRaceStr {
    fn to_race_string(&self) -> String;
}

impl DurationToRaceStr for Duration {
    fn to_race_string(&self) -> String {
        let days = self.as_secs() / (3600 * 24);
        let ms = self.subsec_millis();
        let seconds = self.as_secs() % 60;
        let minutes = (self.as_secs() / 60) % 60;
        let hours = ((self.as_secs() / 60) / 60) % 24;
        format!(
            "{}{}{:0>2}:{:0>2}.{:0>2}",
            if days > 0 {
                format!("{days}d ")
            } else {
                String::default()
            },
            if hours > 0 || days > 0 {
                format!("{hours:0>2}:")
            } else {
                String::default()
            },
            minutes,
            seconds,
            ms / 10
        )
    }
}
