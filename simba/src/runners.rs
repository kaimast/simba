/// This module contains helper classes to run simulation
///
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;

use anyhow::Context;

use fast_float::parse;

use parking_lot::{Condvar, Mutex};

use asim::time::Time;

use crate::config::{
    Constraint, ExperimentConfiguration, FailureConfig, Interval, ParameterType, ParameterValue,
    TestConfiguration,
};
use crate::failures::Failures;
use crate::library::Library;
use crate::message::MessageType;
use crate::metrics::MetricType;
use crate::object::ObjectId;
use crate::simulation::Simulation;

struct MessageLogger {
    file: Mutex<File>,
}

impl MessageLogger {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            file: Mutex::new(File::create("messages.log")?),
        })
    }

    fn write(&self, time: Time, source: &ObjectId, dest: &ObjectId, msg_type: MessageType) {
        let line = format!("[{time}] {source} -> {dest}: {msg_type}\n");
        self.file.lock().write_all(line.as_bytes()).unwrap();
    }
}

/// Runs a specific setup forever
pub struct EndlessRunner {
    simulation: Simulation,
}

impl EndlessRunner {
    pub fn new(
        library_path: &str,
        network_name: &str,
        protocol_name: &str,
        failures: Option<FailureConfig>,
        overwrites: Vec<(ParameterType, ParameterValue)>,
        stats_file: Option<String>,
    ) -> anyhow::Result<Self> {
        let library = Library::new(library_path)?;

        let mut protocol = library.get_protocol(protocol_name)?.clone();
        let mut network = library.get_network(network_name)?.clone();

        let failures = Failures::new(network.num_nodes(), failures);

        for (param, val) in overwrites {
            protocol.set(&param, val);
            network.set(&param, val);
        }

        let simulation = Simulation::new(protocol, network, failures, stats_file)?;

        Ok(Self { simulation })
    }

    /// Spawn simulation in a dedicated task
    /// Will run until stop() is called
    pub fn start(&self) {
        self.simulation.start();
    }

    pub fn stop(&self) {
        self.simulation.stop();
    }

    pub fn run_until_ctrlc(&self) {
        self.start();

        let stop_flag = Arc::new(Mutex::new(false));
        let stop_cond = Arc::new(Condvar::new());

        {
            let stop_flag = stop_flag.clone();
            let stop_cond = stop_cond.clone();

            ctrlc::set_handler(move || {
                let mut flag = stop_flag.lock();
                *flag = true;
                stop_cond.notify_all();
            })
            .expect("Error setting Ctrl-C handler");
        }

        {
            log::info!("Waiting for Ctrl-C...");
            let mut flag = stop_flag.lock();
            while !*flag {
                stop_cond.wait(&mut flag);
            }
        }

        self.stop();
    }
}

/// Runs a test configuration
//TODO better error handling
pub struct TestRunner {
    simulation: Simulation,
    test: TestConfiguration,
}

impl TestRunner {
    pub fn new(
        library_path: &str,
        test_name: &str,
        stats_file: Option<String>,
    ) -> anyhow::Result<Self> {
        let library = Library::new(library_path)?;

        let test = library.get_test(test_name).clone();
        let protocol = library.get_protocol(&test.protocol)?.clone();
        let network = library.get_network(&test.network)?.clone();

        let failures = Failures::none(network.num_nodes());
        let simulation = Simulation::new(protocol, network, failures, stats_file)?;

        Ok(Self { simulation, test })
    }

    pub fn run(&self) -> bool {
        let test = &self.test;
        log::info!("Test set up. Timeout is set to {:?}", test.timeout,);

        let simulation = &self.simulation;

        simulation.run_until(test.timeout);
        let chain_metrics = simulation.get_chain_metrics(test.timeout);

        log::info!("Done.");
        log::info!("Throughput was {} txns/s", chain_metrics.get_throughput());
        log::info!("Blockrate was {} block/s", chain_metrics.get_block_rate());
        log::info!("Winrate was {} blocks/s", chain_metrics.get_win_rate());
        log::info!(
            "Average block interval was {}s",
            chain_metrics.avg_block_interval
        );

        let mut success = true;
        for assert in test.asserts.iter() {
            let value = match assert.metric {
                MetricType::Chain(cmetric) => chain_metrics.get(&cmetric),
                MetricType::Network(nmetric) => simulation.get_network_metric(nmetric),
            };

            match assert.constraint {
                Constraint::InRange { min, max } => {
                    if value < min || value > max {
                        log::error!(
                            "Metric \"{}\" not in range [{min}, {max}]; was {value}",
                            assert.metric
                        );
                        success = false;
                    }
                }
                Constraint::GreaterThan(min) => {
                    if value <= min {
                        log::error!(
                            "Metric \"{}\" is not greater than {min}; was {value}",
                            assert.metric
                        );
                        success = false;
                    }
                }
            }
        }

        success
    }
}

/// Runs a specific experiment
pub struct ExperimentRunner {
    config: ExperimentConfiguration,
    library: Arc<Library>,
    csv_file: Mutex<csv::Writer<File>>,
    parallelism: usize,
    log_messages: bool,
    stats_file: Option<String>,
}

struct IntervalGenerator {
    // Make sure the parameters are printed in the specified order
    data_ranges: Vec<(ParameterType, Interval)>,
    pos: usize,
}

impl IntervalGenerator {
    fn new(data_ranges: Vec<(ParameterType, Interval)>) -> anyhow::Result<Self> {
        let mut param_set = HashSet::new();

        for (param, interval) in data_ranges.iter() {
            assert!(interval.is_valid());
            if !param_set.insert(param) {
                anyhow::bail!("Parameter {param} was specified more than once!");
            }
        }

        if data_ranges.is_empty() {
            anyhow::bail!("No data ranges specified. Will run nothing!");
        }

        Ok(Self {
            data_ranges,
            pos: 0,
        })
    }

    /// Returns the total number of steps in this experiment
    fn num_steps(&self) -> usize {
        let mut result = 1;
        for (_, interval) in self.data_ranges.iter() {
            result *= interval.num_steps();
        }
        result
    }

    /// Gets the step at the specified index
    fn get_step(&self, mut idx: usize) -> Option<Vec<(ParameterType, ParameterValue)>> {
        assert!(idx < self.num_steps());
        let mut result = vec![];

        for (key, interval) in self.data_ranges.iter() {
            let offset = idx % interval.num_steps();
            idx = idx / interval.num_steps();
            let val = interval.get_step(offset)?;
            result.push((*key, val));
        }
        Some(result)
    }

    /// Gets the next step (if any) or returns None
    fn get_next(&mut self) -> Option<Vec<(ParameterType, ParameterValue)>> {
        // First, check if we're done
        if self.pos >= self.num_steps() {
            return None;
        }

        let result = self.get_step(self.pos);
        assert!(result.is_some());
        self.pos += 1;
        result
    }
}

impl ExperimentRunner {
    pub fn new(
        library_path: &str,
        exp_name: &str,
        parallelism: Option<usize>,
        stats_file: Option<String>,
        log_messages: bool,
    ) -> anyhow::Result<Self> {
        let library = Arc::new(Library::new(library_path)?);

        let parallelism = if let Some(num) = parallelism {
            num
        } else {
            num_cpus::get()
        };

        let config = library.get_experiment(exp_name).clone();

        let mut csv_file = csv::Writer::from_path(format!("results-{exp_name}.csv"))
            .expect("Failed to open CSV file to write to");

        let mut record = vec![];
        for (key, _) in config.data_ranges.iter() {
            record.push(format!("{key}"));
        }

        for metric in config.metrics.iter() {
            record.push(format!("{metric}"));
        }

        // Write header
        csv_file
            .write_record(&record)
            .expect("Failed to write to CSV file");

        let csv_file = Mutex::new(csv_file);

        Ok(Self {
            config,
            csv_file,
            library,
            parallelism,
            stats_file,
            log_messages,
        })
    }

    pub fn single_step(&self, index: usize) -> anyhow::Result<()> {
        let config = self.config.clone();
        let library = self.library.clone();
        let name = &self.config.protocol;

        let generator = IntervalGenerator::new(self.config.data_ranges.clone())?;
        let value = generator.get_step(index).expect("Index out of range");

        let record = Self::run_next(
            &library,
            &config,
            value,
            self.stats_file.clone(),
            self.log_messages,
        )?;
        self.write_record(record)?;
        let mut csv_file = csv::Reader::from_path(format!("results-{name}.csv"))
            .expect("Failed to open CSV file to read from");
        let mut throughput: Vec<f64> = Vec::new();
        let mut latency = vec![];
        for result in csv_file.records() {
            let record = result.expect("Failed to parse");
            if let Some(i) = record.get(0) {
                let tmp: f64 = parse(i).unwrap();
                throughput.push(tmp)
            }
            if let Some(i) = record.get(1) {
                let tmp: f64 = parse(i).unwrap();
                latency.push(tmp)
            }
        }

        Ok(())
    }

    pub fn run(&self) -> anyhow::Result<()> {
        log::info!("Running up to {} experiments in parallel", self.parallelism);

        let parallelism = self.parallelism;
        let config = self.config.clone();
        let library = self.library.clone();

        let mut generator = IntervalGenerator::new(self.config.data_ranges.clone())?;
        let mut at_end = false;

        while !at_end {
            let mut tasks = vec![];

            while tasks.len() < parallelism {
                let next_value = match generator.get_next() {
                    Some(val) => val,
                    None => {
                        at_end = true;
                        break;
                    }
                };

                let hdl = {
                    let library = library.clone();
                    let config = config.clone();
                    let log_messages = self.log_messages;
                    let stats_file = self.stats_file.clone();

                    std::thread::spawn(move || {
                        Self::run_next(&library, &config, next_value, stats_file, log_messages)
                    })
                };

                tasks.push(hdl);
            }

            log::info!("Spawned {} concurrent experiments", tasks.len());

            assert!(!tasks.is_empty());
            for hdl in tasks.into_iter() {
                let record = hdl.join().expect("Experiment failed")?;
                self.write_record(record)?;
            }
        }

        Ok(())
    }

    fn write_record(&self, record: Vec<String>) -> anyhow::Result<()> {
        let mut csv_file = self.csv_file.lock();

        if let Err(err) = csv_file.write_record(&record) {
            anyhow::bail!("Failed to write to CSV file: {err}");
        }
        csv_file.flush().unwrap();
        Ok(())
    }

    fn run_next(
        library: &Library,
        config: &ExperimentConfiguration,
        params: Vec<(ParameterType, ParameterValue)>,
        stats_file: Option<String>,
        log_messages: bool,
    ) -> anyhow::Result<Vec<String>> {
        let mut protocol = library.get_protocol(&config.protocol)?.clone();
        let mut network = library.get_network(&config.network)?.clone();

        // Parameters will either modify the network or the protocol configuration
        for (param_type, value) in params.iter() {
            protocol.set(param_type, *value);
            network.set(param_type, *value);
        }

        log::info!("Running next step with {params:#?}");

        let failures = Failures::new(network.num_nodes(), config.failures.clone());
        let simulation = Simulation::new(protocol, network, failures, stats_file)
            .with_context(|| "Failed to initialize simulation")?;

        if log_messages {
            let logger = MessageLogger::new()?;
            simulation.set_message_sent_event_callback(Box::new(
                move |time, src, dst, msg_type| {
                    logger.write(time, &src, &dst, msg_type);
                },
            ));
        }

        simulation.run_until(config.timeout);
        let metrics = simulation.get_chain_metrics(config.timeout);

        let mut record = vec![];
        for (_, value) in params.iter() {
            record.push(format!("{value}"));
        }

        for chain_metric in config.metrics.iter() {
            let value = metrics.get(chain_metric);
            record.push(format!("{value}"));
        }

        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Interval, ParameterType, ParameterValue};

    use super::IntervalGenerator;

    #[test]
    fn interval_generator() {
        let mut params = vec![];
        params.push((ParameterType::NumClients, Interval::LinearInt {
            start: 10,
            end: 60,
            step_size: 10,
        }));
        params.push((ParameterType::NumMiningNodes, Interval::LinearFloat {
            start: 0.2,
            end: 1.0,
            step_size: 0.2,
        }));

        let generator = IntervalGenerator::new(params).unwrap();
        let first_step = generator.get_step(0).unwrap();

        assert_eq!(first_step[0].1, ParameterValue::Int(10));
        assert_eq!(first_step[1].1, ParameterValue::Float(0.2));

        let last_step = generator.get_step(6 * 5 - 1).unwrap();

        assert_eq!(last_step[0].1, ParameterValue::Int(60),);
        assert_eq!(last_step[1].1, ParameterValue::Float(1.0));
    }
}
