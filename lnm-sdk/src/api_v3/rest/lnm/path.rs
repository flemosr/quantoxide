use crate::shared::rest::lnm::base::RestPath;

#[derive(Clone)]
pub(in crate::api_v3) enum RestPathV3 {
    FuturesIsolatedTrade,
    FuturesIsolatedTradeAddMargin,
    FuturesIsolatedTradeCancel,
    FuturesIsolatedTradeClose,
    FuturesIsolatedTradeTakeprofit,
    FuturesIsolatedTradeStoploss,
    FuturesIsolatedTradesCancelAll,
    FuturesIsolatedTradesOpen,
    FuturesIsolatedTradesRunning,
    FuturesIsolatedTradesClosed,
    FuturesIsolatedTradesCanceled,
    FuturesDataTicker,
}

impl RestPath for RestPathV3 {
    fn to_path_string(self) -> String {
        match self {
            RestPathV3::FuturesIsolatedTrade => "/v3/futures/isolated/trade".into(),
            RestPathV3::FuturesIsolatedTradeAddMargin => {
                "/v3/futures/isolated/trade/add-margin".into()
            }
            RestPathV3::FuturesIsolatedTradeCancel => "/v3/futures/isolated/trade/cancel".into(),
            RestPathV3::FuturesIsolatedTradeClose => "/v3/futures/isolated/trade/close".into(),
            RestPathV3::FuturesIsolatedTradeTakeprofit => {
                "/v3/futures/isolated/trade/takeprofit".into()
            }
            RestPathV3::FuturesIsolatedTradeStoploss => {
                "/v3/futures/isolated/trade/stoploss".into()
            }
            RestPathV3::FuturesIsolatedTradesCancelAll => {
                "/v3/futures/isolated/trades/cancel-all".into()
            }
            RestPathV3::FuturesIsolatedTradesOpen => "/v3/futures/isolated/trades/open".into(),
            RestPathV3::FuturesIsolatedTradesRunning => {
                "/v3/futures/isolated/trades/running".into()
            }
            RestPathV3::FuturesIsolatedTradesClosed => "/v3/futures/isolated/trades/closed".into(),
            RestPathV3::FuturesIsolatedTradesCanceled => {
                "/v3/futures/isolated/trades/canceled".into()
            }
            RestPathV3::FuturesDataTicker => "/v3/futures/ticker".into(),
        }
    }
}
