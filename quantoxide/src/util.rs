use chrono::{DateTime, Duration, SubsecRound, Utc};

pub trait CeilSec {
    fn ceil_sec(&self) -> DateTime<Utc>;
}

impl CeilSec for DateTime<Utc> {
    fn ceil_sec(&self) -> DateTime<Utc> {
        let trunc_time_sec = self.trunc_subsecs(0);
        if trunc_time_sec == *self {
            trunc_time_sec
        } else {
            trunc_time_sec + Duration::seconds(1)
        }
    }
}
