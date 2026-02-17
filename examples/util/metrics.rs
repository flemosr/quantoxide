//! Financial metrics for backtest evaluation.

#![allow(unused)]

/// Calculate annualized Sharpe ratio from daily net values.
/// Assumes 365 trading days per year. Returns `None` if there is insufficient data.
pub fn calculate_sharpe_ratio(daily_values: &[f64], annual_risk_free_rate: f64) -> Option<f64> {
    if daily_values.len() < 3 {
        return None;
    }

    let returns: Vec<f64> = daily_values
        .windows(2)
        .filter(|w| w[0] != 0.0)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    if returns.len() < 2 {
        return None;
    }

    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let daily_risk_free_rate = annual_risk_free_rate / 365.0;
    let excess_return = mean_return - daily_risk_free_rate;

    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return None;
    }

    // Annualize
    Some((excess_return / std_dev) * 365.0_f64.sqrt())
}

/// Calculate maximum drawdown from daily net values.
/// Returns the maximum peak-to-trough decline as a fraction. Returns `None` if there is
/// insufficient data.
pub fn calculate_max_drawdown(daily_values: &[f64]) -> Option<f64> {
    if daily_values.len() < 2 {
        return None;
    }

    let mut peak = daily_values[0];
    let mut max_drawdown = 0.0_f64;

    for &value in &daily_values[1..] {
        if value > peak {
            peak = value;
        }
        if peak > 0.0 {
            let drawdown = (peak - value) / peak;
            max_drawdown = max_drawdown.max(drawdown);
        }
    }

    Some(max_drawdown)
}
