mod error;

pub mod core;

mod manager;

mod simulation;

pub mod live;

pub use manager::LiveTradesManager;

pub use simulation::SimulatedTradesManager;
