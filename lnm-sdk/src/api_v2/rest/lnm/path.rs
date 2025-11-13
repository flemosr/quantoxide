use uuid::Uuid;

use crate::shared::rest::lnm::base::ApiPath;

#[derive(Clone)]
pub(in crate::api_v2) enum ApiPathV2 {
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

impl ApiPath for ApiPathV2 {
    fn to_path_string(self) -> String {
        match self {
            ApiPathV2::FuturesPriceHistory => "/v2/futures/history/price".into(),
            ApiPathV2::FuturesTrade => "/v2/futures".into(),
            ApiPathV2::FuturesGetTrade(id) => format!("/v2/futures/trades/{id}"),
            ApiPathV2::FuturesTicker => "/v2/futures/ticker".into(),
            ApiPathV2::FuturesCancelTrade => "/v2/futures/cancel".into(),
            ApiPathV2::FuturesCancelAllTrades => "/v2/futures/all/cancel".into(),
            ApiPathV2::FuturesCloseAllTrades => "/v2/futures/all/close".into(),
            ApiPathV2::FuturesAddMargin => "/v2/futures/add-margin".into(),
            ApiPathV2::FuturesCashIn => "/v2/futures/cash-in".into(),
            ApiPathV2::UserGetUser => "/v2/user".into(),
        }
    }
}
