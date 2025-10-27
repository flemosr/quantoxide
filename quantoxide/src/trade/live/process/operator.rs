use std::sync::Arc;

use crate::{
    db::DbContext,
    signal::engine::{LiveSignalController, LiveSignalEngine},
    sync::SyncReader,
    trade::core::TradeExecutor,
};

use super::{
    super::super::core::{WrappedRawOperator, WrappedSignalOperator},
    error::{LiveProcessFatalError, LiveProcessFatalResult as Result},
};

pub enum OperatorPending {
    Signal {
        signal_engine: LiveSignalEngine,
        signal_operator: WrappedSignalOperator,
    },
    Raw {
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl OperatorPending {
    pub fn signal(signal_engine: LiveSignalEngine, signal_operator: WrappedSignalOperator) -> Self {
        Self::Signal {
            signal_engine,
            signal_operator,
        }
    }

    pub fn raw(
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    ) -> Self {
        Self::Raw {
            db,
            sync_reader,
            raw_operator,
        }
    }

    pub async fn start(self, trade_executor: Arc<dyn TradeExecutor>) -> Result<OperatorRunning> {
        match self {
            OperatorPending::Signal {
                signal_engine,
                mut signal_operator,
            } => {
                signal_operator
                    .set_trade_executor(trade_executor.clone())
                    .map_err(LiveProcessFatalError::StartOperatorError)?;

                let signal_controller = signal_engine.start();

                Ok(OperatorRunning::Signal {
                    signal_operator,
                    signal_controller,
                })
            }
            OperatorPending::Raw {
                db,
                sync_reader,
                mut raw_operator,
            } => {
                raw_operator
                    .set_trade_executor(trade_executor.clone())
                    .map_err(LiveProcessFatalError::StartOperatorError)?;

                Ok(OperatorRunning::Raw {
                    db,
                    sync_reader,
                    raw_operator,
                })
            }
        }
    }
}

pub enum OperatorRunning {
    Signal {
        signal_controller: Arc<LiveSignalController>,
        signal_operator: WrappedSignalOperator,
    },
    Raw {
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        raw_operator: WrappedRawOperator,
    },
}

impl OperatorRunning {
    pub fn signal_controller(&self) -> Option<Arc<LiveSignalController>> {
        if let OperatorRunning::Signal {
            signal_operator: _,
            signal_controller,
        } = self
        {
            Some(signal_controller.clone())
        } else {
            None
        }
    }
}
