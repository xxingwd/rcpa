use std::collections::HashMap;

use crate::config::CostConfig;

/// Cost calculator using per-model pricing
pub struct CostCalculator {
    currency: String,
    default_input_per_1k: f64,
    default_output_per_1k: f64,
    models: HashMap<String, (f64, f64)>, // (input_per_1k, output_per_1k)
}

impl CostCalculator {
    pub fn from_config(config: &CostConfig) -> Self {
        let models = config
            .models
            .iter()
            .map(|(name, pricing)| (name.clone(), (pricing.input_per_1k, pricing.output_per_1k)))
            .collect();

        Self {
            currency: config.currency.clone(),
            default_input_per_1k: config.default_input_per_1k,
            default_output_per_1k: config.default_output_per_1k,
            models,
        }
    }

    /// Calculate cost in the configured currency (stored as integer cents)
    pub fn calculate(&self, model: &str, prompt_tokens: u64, completion_tokens: u64) -> u64 {
        let (input_rate, output_rate) = self
            .models
            .get(model)
            .copied()
            .unwrap_or((self.default_input_per_1k, self.default_output_per_1k));

        let input_cost = (prompt_tokens as f64 / 1000.0) * input_rate;
        let output_cost = (completion_tokens as f64 / 1000.0) * output_rate;

        // Convert to cents (integer for atomic operations)
        let cents = (input_cost + output_cost) * 100.0;
        if cents > 0.0 && cents < 1.0 {
            1
        } else {
            cents.round() as u64
        }
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }
}
