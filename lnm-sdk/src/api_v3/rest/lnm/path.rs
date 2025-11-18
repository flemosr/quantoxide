use crate::shared::rest::lnm::base::RestPath;

#[derive(Clone)]
pub(in crate::api_v3) enum RestPathV3 {
    FuturesIsolatedTrade,
    FuturesIsolatedTradeAddMargin,
    FuturesIsolatedTradeCancel,
    FuturesIsolatedTradeCashIn,
    FuturesIsolatedTradeClose,
    FuturesIsolatedTradeTakeprofit,
    FuturesIsolatedTradeStoploss,
    FuturesIsolatedTradesCancelAll,
    FuturesIsolatedTradesOpen,
    FuturesIsolatedTradesRunning,
    FuturesIsolatedTradesClosed,
    FuturesIsolatedTradesCanceled,
    FuturesIsolatedFundingFees,
    FuturesCrossOrder,
    FuturesCrossOrderCancel,
    FuturesCrossOrdersCancelAll,
    FuturesCrossOrdersOpen,
    FuturesCrossOrdersFilled,
    FuturesCrossPosition,
    FuturesCrossPositionClose,
    FuturesCrossPositionSetLeverage,
    FuturesCrossDeposit,
    FuturesCrossWithdraw,
    FuturesCrossGetTransfers,
    FuturesCrossFundingFees,
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
            RestPathV3::FuturesIsolatedTradeCashIn => "/v3/futures/isolated/trade/cash-in".into(),
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
            RestPathV3::FuturesIsolatedFundingFees => "/v3/futures/isolated/funding-fees".into(),
            RestPathV3::FuturesCrossOrder => "/v3/futures/cross/order".into(),
            RestPathV3::FuturesCrossOrderCancel => "/v3/futures/cross/order/cancel".into(),
            RestPathV3::FuturesCrossOrdersCancelAll => "/v3/futures/cross/orders/cancel-all".into(),
            RestPathV3::FuturesCrossOrdersOpen => "/v3/futures/cross/orders/open".into(),
            RestPathV3::FuturesCrossOrdersFilled => "/v3/futures/cross/orders/filled".into(),
            RestPathV3::FuturesCrossPosition => "/v3/futures/cross/position".into(),
            RestPathV3::FuturesCrossPositionClose => "/v3/futures/cross/position/close".into(),
            RestPathV3::FuturesCrossPositionSetLeverage => "/v3/futures/cross/leverage".into(),
            RestPathV3::FuturesCrossDeposit => "/v3/futures/cross/deposit".into(),
            RestPathV3::FuturesCrossWithdraw => "/v3/futures/cross/withdraw".into(),
            RestPathV3::FuturesCrossGetTransfers => "/v3/futures/cross/transfers".into(),
            RestPathV3::FuturesCrossFundingFees => "/v3/futures/cross/funding-fees".into(),
            RestPathV3::FuturesDataTicker => "/v3/futures/ticker".into(),
        }
    }
}
