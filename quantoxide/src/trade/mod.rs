mod error;

pub mod core;

mod live;

mod simulation;

pub use live::LiveTradesManager;

pub use simulation::SimulatedTradesManager;
