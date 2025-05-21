use chrono::{DateTime, Duration, SubsecRound, Utc};

/// A type that can not be instantiated
pub enum Never {}

pub trait DateTimeExt {
    fn ceil_sec(&self) -> DateTime<Utc>;

    fn is_round(&self) -> bool;
}

impl DateTimeExt for DateTime<Utc> {
    fn ceil_sec(&self) -> DateTime<Utc> {
        let trunc_time_sec = self.trunc_subsecs(0);
        if trunc_time_sec == *self {
            trunc_time_sec
        } else {
            trunc_time_sec + Duration::seconds(1)
        }
    }

    fn is_round(&self) -> bool {
        *self == self.trunc_subsecs(0)
    }
}
