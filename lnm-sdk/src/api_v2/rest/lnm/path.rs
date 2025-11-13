use uuid::Uuid;

use crate::shared::rest::lnm::base::RestPath;

#[derive(Clone)]
pub(in crate::api_v2) enum RestPathV2 {
    FuturesPriceHistory,
    FuturesTrade,
    FuturesGetTrade(Uuid),
    FuturesTicker,
    FuturesCancelTrade,
    FuturesCancelAllTrades,
    FuturesCloseAllTrades,
    FuturesAddMargin,
    FuturesCashIn,
    UserGetUser,
}

impl RestPath for RestPathV2 {
    fn to_path_string(self) -> String {
        match self {
            RestPathV2::FuturesPriceHistory => "/v2/futures/history/price".into(),
            RestPathV2::FuturesTrade => "/v2/futures".into(),
            RestPathV2::FuturesGetTrade(id) => format!("/v2/futures/trades/{id}"),
            RestPathV2::FuturesTicker => "/v2/futures/ticker".into(),
            RestPathV2::FuturesCancelTrade => "/v2/futures/cancel".into(),
            RestPathV2::FuturesCancelAllTrades => "/v2/futures/all/cancel".into(),
            RestPathV2::FuturesCloseAllTrades => "/v2/futures/all/close".into(),
            RestPathV2::FuturesAddMargin => "/v2/futures/add-margin".into(),
            RestPathV2::FuturesCashIn => "/v2/futures/cash-in".into(),
            RestPathV2::UserGetUser => "/v2/user".into(),
        }
    }
}
