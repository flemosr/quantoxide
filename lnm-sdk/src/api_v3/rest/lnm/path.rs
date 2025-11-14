use crate::shared::rest::lnm::base::RestPath;

#[derive(Clone)]
pub(in crate::api_v3) enum RestPathV3 {
    FuturesIsolatedTrade,
    FuturesIsolatedTradesCancelAll,
    FuturesIsolatedTradesOpen,
    FuturesDataTicker,
}

impl RestPath for RestPathV3 {
    fn to_path_string(self) -> String {
        match self {
            RestPathV3::FuturesIsolatedTrade => "/v3/futures/isolated/trade".into(),
            RestPathV3::FuturesIsolatedTradesCancelAll => {
                "/v3/futures/isolated/trades/cancel-all".into()
            }
            RestPathV3::FuturesIsolatedTradesOpen => "/v3/futures/isolated/trades/open".into(),
            RestPathV3::FuturesDataTicker => "/v3/futures/ticker".into(),
        }
    }
}
