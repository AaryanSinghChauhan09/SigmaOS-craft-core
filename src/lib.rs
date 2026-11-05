//! Shared project format, asset metadata, import/export, plugin API
//!
//! Subsystem component of SigmaOS following core architectural principles:
//! DRY, KISS, SOLID, AAA testing, Separation of Concerns, Gall's Law.

/// Configuration contract for SigmaOS-craft-core
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub component_name: String,
    pub tier: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            component_name: String::from("SigmaOS-craft-core"),
            tier: 4,
        }
    }
}

/// State descriptor for SigmaOS-craft-core
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub initialized: bool,
    pub healthy: bool,
}

/// Lifecycle supervisor for SigmaOS-craft-core
pub struct Supervisor {
    config: Config,
    state: State,
}

impl Supervisor {
    /// Create a new supervisor instance
    pub fn new(config: Config) -> Self {
        Supervisor {
            config,
            state: State {
                initialized: false,
                healthy: true,
            },
        }
    }

    /// Start and initialize the component
    pub fn initialize(&mut self) -> Result<String, String> {
        if !self.config.enabled {
            return Err(String::from("Component is administratively disabled"));
        }
        self.state.initialized = true;
        Ok(format!("{} initialized successfully at tier {}", self.config.component_name, self.config.tier))
    }

    /// Query current health
    pub fn is_healthy(&self) -> bool {
        self.state.initialized && self.state.healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configuration() {
        // Arrange
        let config = Config::default();

        // Act
        let is_enabled = config.enabled;
        let tier_val = config.tier;

        // Assert
        assert!(is_enabled, "component must be enabled by default");
        assert_eq!(tier_val, 4, "tier must match component contract");
    }

    #[test]
    fn test_successful_initialization() {
        // Arrange
        let config = Config::default();
        let mut supervisor = Supervisor::new(config);

        // Act
        let result = supervisor.initialize();

        // Assert
        assert!(result.is_ok(), "initialization should succeed with valid config");
        assert!(supervisor.is_healthy(), "component should report healthy state after init");
    }

    #[test]
    fn test_disabled_initialization_failure() {
        // Arrange
        let config = Config {
            enabled: false,
            component_name: String::from("SigmaOS-craft-core"),
            tier: 4,
        };
        let mut supervisor = Supervisor::new(config);

        // Act
        let result = supervisor.initialize();

        // Assert
        assert!(result.is_err(), "disabled component should reject initialization");
        assert!(!supervisor.is_healthy(), "disabled component must not be marked healthy");
    }
}
