use simba::{EndlessRunner, ExperimentRunner, Library, TestRunner};

use clap::Parser;

use tracing_flame::FlameLayer;
use tracing_subscriber::{filter::EnvFilter, prelude::*};

fn setup_tracing() -> impl Drop {
    let fmt_layer = EnvFilter::from_default_env();
    let (flame_layer, _guard) = FlameLayer::with_file("./simba-trace.folded").unwrap();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(flame_layer)
        .init();
    _guard
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    mode: Mode,

    #[clap(long, short = 'p', global = true, default_value = "./library")]
    #[clap(help = "Where to look for the configuration files?")]
    library_path: String,

    #[clap(long, global = true)]
    #[clap(help = "Should we collect tracing data?")]
    enable_tracing: bool,

    #[clap(long, short = 'j', global = true, required = false)]
    #[clap(help = "How many concurrent tasks? Will be the number of cores by default")]
    parallelism: Option<usize>,

    #[clap(long, short = 's', global = true)]
    #[clap(help = "Generate a CSV file with simulation statics")]
    log_stats: bool,

    #[clap(long, global = true, default_value = "statistics.csv")]
    #[clap(help = "The name of the statistics file. This only matters if --log-stats is set.")]
    stats_filename: String,
}

#[derive(clap::Subcommand)]
enum Mode {
    #[clap(about = "Run or inspect experiments")]
    Experiment {
        #[clap(subcommand)]
        command: ExpCommand,
    },
    #[clap(about = "Run any combination of network and protocol")]
    Endless {
        #[clap(help = "The name of network topology to use")]
        network_name: String,
        #[clap(help = "The name of protocol to use")]
        protocol_name: String,
        #[clap(
            long,
            short = 'o',
            help = "Overwrite parameter of the network or protocol"
        )]
        overwrite: Vec<String>,
    },
    Test {
        #[clap(help = "The name of the test to run")]
        test_name: String,
    },
    ListNetworks,
    ListProtocols,
}

#[derive(clap::Subcommand)]
enum ExpCommand {
    CountSteps {
        #[clap(help = "The name of the experiment to inspect")]
        experiment_name: String,
    },
    #[clap(about = "Run an experiment and output a CSV file")]
    Run {
        #[clap(help = "The name of the experiment to run")]
        experiment_name: String,
        #[clap(long, help = "Writes all messages to a messages.log file")]
        log_messages: bool,
    },
    #[clap(about = "Run a single step of an experiment and output a CSV file")]
    SingleStep {
        #[clap(help = "The name of the experiment to run")]
        experiment_name: String,
        #[clap(help = "The index of the step")]
        index: usize,
        #[clap(long, help = "Writes all messages to a messages.log file")]
        log_messages: bool,
    },
    #[clap(about = "Lists all experiments")]
    List,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _tracing_guard = if args.enable_tracing {
        Some(setup_tracing())
    } else {
        env_logger::init();
        None
    };

    let stats_file = if args.log_stats {
        Some(args.stats_filename)
    } else {
        None
    };

    match args.mode {
        Mode::Experiment { command } => match command {
            ExpCommand::Run {
                experiment_name,
                log_messages,
            } => {
                let runner = match ExperimentRunner::new(
                    &args.library_path,
                    &experiment_name,
                    args.parallelism,
                    stats_file,
                    log_messages,
                ) {
                    Ok(runner) => runner,
                    Err(err) => {
                        log::error!("Failed to run command: {err}");
                        std::process::exit(-1);
                    }
                };

                #[cfg(feature = "cpuprofiler")]
                cpuprofiler::PROFILER
                    .lock()
                    .unwrap()
                    .start("./simba.profile")
                    .unwrap();
                runner.run()?;
                #[cfg(feature = "cpuprofiler")]
                cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();
            }
            ExpCommand::CountSteps { experiment_name } => {
                let library = Library::new(&args.library_path).expect("Failed to open library");
                let exp = library.get_experiment(&experiment_name);

                println!("{}", exp.num_steps());
            }
            ExpCommand::SingleStep {
                experiment_name,
                index,
                log_messages,
            } => {
                let runner = ExperimentRunner::new(
                    &args.library_path,
                    &experiment_name,
                    args.parallelism,
                    stats_file,
                    log_messages,
                )?;

                #[cfg(feature = "cpuprofiler")]
                cpuprofiler::PROFILER
                    .lock()
                    .unwrap()
                    .start("./simba.profile")
                    .unwrap();
                runner.single_step(index)?;
                #[cfg(feature = "cpuprofiler")]
                cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();
            }
            ExpCommand::List => {
                let library = Library::new(&args.library_path)?;
                print!("Found experiments: {:?}", library.get_experiment_names());
            }
        },
        Mode::Endless {
            network_name,
            protocol_name,
            overwrite,
        } => {
            let mut overwrites = vec![];

            for arg in overwrite {
                let mut split = arg.split('=');
                let param = split
                    .next()
                    .expect("Invalid overwrite argument")
                    .try_into()
                    .expect("Failed to parse parameter");
                let value = split
                    .next()
                    .expect("Invalid overwrite argument")
                    .try_into()
                    .expect("Failed to parse value");
                if split.next().is_some() {
                    panic!("Invalid overwrite argument");
                }
                overwrites.push((param, value));
            }

            let runner = EndlessRunner::new(
                &args.library_path,
                &network_name,
                &protocol_name,
                None,
                overwrites,
                stats_file,
            )?;

            runner.run_until_ctrlc();
        }
        Mode::Test { test_name } => {
            let runner = match TestRunner::new(&args.library_path, &test_name, stats_file) {
                Ok(runner) => runner,
                Err(err) => {
                    log::error!("Failed to run test: {err}");
                    std::process::exit(-1);
                }
            };

            #[cfg(feature = "cpuprofiler")]
            cpuprofiler::PROFILER
                .lock()
                .unwrap()
                .start("./simba.profile")
                .unwrap();
            let result = runner.run();
            #[cfg(feature = "cpuprofiler")]
            cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();

            if !result {
                std::process::exit(1);
            }
        }
        Mode::ListNetworks => {
            let library = Library::new(&args.library_path)?;
            print!("Found networks: {:?}", library.get_network_names());
        }
        Mode::ListProtocols => {
            let library = Library::new(&args.library_path)?;
            print!("Found protocols: {:?}", library.get_protocol_names());
        }
    }

    Ok(())
}
