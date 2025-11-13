use crate::shared::rest::lnm::base::RestPath;

#[derive(Clone)]
pub(in crate::api_v3) enum RestPathV3 {
    FuturesIsolatedTrade,
}

impl RestPath for RestPathV3 {
    fn to_path_string(self) -> String {
        match self {
            RestPathV3::FuturesIsolatedTrade => "/v3/futures/isolated/trade".into(),
        }
    }
}
