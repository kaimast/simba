use std::collections::HashMap;
use std::fs::{File, read_dir};
use std::path::Path;

use crate::{
    ExperimentConfiguration, NetworkConfiguration, ProtocolConfiguration, TestConfiguration,
};

pub struct Library {
    protocols: HashMap<String, ProtocolConfiguration>,
    networks: HashMap<String, NetworkConfiguration>,
    tests: HashMap<String, TestConfiguration>,
    experiments: HashMap<String, ExperimentConfiguration>,
}

impl Library {
    fn read_config_files<T: serde::de::DeserializeOwned>(
        base_path: &Path,
        subdir: &str,
    ) -> anyhow::Result<HashMap<String, T>> {
        let mut result: HashMap<String, T> = Default::default();
        let dir_path = base_path.join(Path::new(subdir));

        let directory = match read_dir(&dir_path) {
            Ok(dir) => dir,
            Err(err) => {
                anyhow::bail!("Failed to open configuration directory {dir_path:?}: {err}");
            }
        };

        for entry in directory {
            let entry = entry.expect("Failed to read next file");
            let file_path = entry.path();

            if !entry.file_type().unwrap().is_file() {
                log::warn!("Entry {file_path:?} is not a regular file. Skipping...");
                continue;
            }

            if file_path.extension().is_none() || file_path.extension().unwrap() != "ron" {
                log::warn!(
                    "Entry {file_path:?} is not a rust object notation (RON) file. Skipping..."
                );
                continue;
            }

            let name: String = file_path
                .file_stem()
                .expect("Invalid file name")
                .to_str()
                .unwrap()
                .to_string();

            let file = match File::open(file_path.clone()) {
                Ok(file) => file,
                Err(err) => anyhow::bail!("Failed to open file: {err:?}"),
            };

            let config: T = match ron::de::from_reader(file) {
                Ok(config) => config,
                Err(err) => {
                    log::error!("Failed to parse RON file at {file_path:?}: {err}. Skipping...");
                    continue;
                }
            };

            result.insert(name, config);
        }

        Ok(result)
    }

    pub fn new<P: AsRef<Path>>(base_path: P) -> anyhow::Result<Self> {
        let base_path: &Path = base_path.as_ref();

        log::info!("Looking for configuration files in {base_path:?}");

        let protocols = Self::read_config_files(base_path, "protocols")?;
        let networks = Self::read_config_files(base_path, "networks")?;
        let tests = Self::read_config_files(base_path, "tests")?;
        let experiments = Self::read_config_files(base_path, "experiments")?;

        log::info!("Found protocol configurations: {:?}", protocols.keys());
        log::info!("Found network configurations: {:?}", networks.keys());

        Ok(Self {
            protocols,
            networks,
            tests,
            experiments,
        })
    }

    pub fn get_protocol(&self, name: &str) -> anyhow::Result<&ProtocolConfiguration> {
        match self.protocols.get(name) {
            Some(proto) => Ok(proto),
            None => {
                anyhow::bail!(
                    "No such protocol \"{name}\", found {:?}",
                    self.get_protocol_names()
                );
            }
        }
    }

    pub fn get_network(&self, name: &str) -> anyhow::Result<&NetworkConfiguration> {
        match self.networks.get(name) {
            Some(proto) => Ok(proto),
            None => {
                anyhow::bail!(
                    "No such network \"{name}\", found {:?}",
                    self.get_network_names()
                );
            }
        }
    }

    pub fn get_experiment(&self, name: &str) -> &ExperimentConfiguration {
        match self.experiments.get(name) {
            Some(config) => config,
            None => {
                panic!(
                    "No such experiment \"{name}\"; found {:?}",
                    self.get_experiment_names()
                );
            }
        }
    }

    pub fn get_test(&self, name: &str) -> &TestConfiguration {
        match self.tests.get(name) {
            Some(test) => test,
            None => {
                panic!("No such test \"{name}\"; found {:?}", self.get_test_names());
            }
        }
    }

    pub fn get_test_names(&self) -> Vec<&str> {
        self.tests.keys().map(|k| k.as_str()).collect()
    }

    pub fn get_experiment_names(&self) -> Vec<&str> {
        self.experiments.keys().map(|k| k.as_str()).collect()
    }

    pub fn get_network_names(&self) -> Vec<&str> {
        self.networks.keys().map(|k| k.as_str()).collect()
    }

    pub fn get_protocol_names(&self) -> Vec<&str> {
        self.protocols.keys().map(|k| k.as_str()).collect()
    }
}
