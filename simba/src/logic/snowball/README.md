# Enhanced Snowball Consensus Implementation

This directory contains an improved implementation of the Snowball consensus protocol, which is the core consensus mechanism used in the Avalanche network.

## Overview

Snowball is a leaderless, Byzantine fault-tolerant consensus protocol that achieves consensus through repeated sampling and voting. The enhanced implementation includes several improvements over the basic version:

## Key Features

### 1. Improved Performance
- **Heartbeat Mechanism**: Periodic liveness detection to identify failed nodes
- **Configurable Timeouts**: Set maximum rounds and heartbeat intervals
- **Better Error Handling**: Comprehensive error handling and validation
- **Optimized Sampling**: Improved peer sampling with fallback mechanisms

### 2. Enhanced Monitoring and Debugging
- **Comprehensive Statistics**: Track rounds, queries, responses, and color changes
- **Decision Time Tracking**: Monitor how long consensus takes to reach
- **Enhanced Logging**: Better observability with structured logging
- **Performance Metrics**: Network message counts and latency tracking

### 3. Configuration Flexibility
- **Heartbeat Interval**: Configure how often nodes send liveness signals
- **Maximum Rounds**: Set upper bound on consensus rounds
- **Network Topology Support**: Better handling of sparse vs. full connectivity

## Configuration Parameters

### Basic Parameters
- `acceptance_threshold` (β): Number of consecutive rounds for acceptance
- `sample_size_weighted` (k/n): Fraction of nodes to sample in each round
- `query_threshold_weighted` (α/k): Fraction of sampled nodes needed for quorum

### Enhanced Parameters
- `heartbeat_interval`: Milliseconds between heartbeat messages
- `max_rounds`: Maximum number of rounds before giving up

## Message Types

### Core Messages
- `Query(Color)`: Request for a node's current color preference
- `QueryResponse(Color)`: Response with current color preference

### Enhanced Messages
- `Heartbeat`: Liveness detection message
- `HeartbeatResponse`: Response to heartbeat

## Usage Examples

### Basic Configuration
```rust
let logic = SnowballGlobalLogic::instantiate(
    100,    // num_nodes
    10,     // acceptance_threshold
    0.25,   // sample_size_weighted
    0.6,    // query_threshold_weighted
);
```

### Enhanced Configuration
```rust
let logic = SnowballGlobalLogic::instantiate_enhanced(
    100,    // num_nodes
    15,     // acceptance_threshold
    0.3,    // sample_size_weighted
    0.7,    // query_threshold_weighted
    800,    // heartbeat_interval
    1200,   // max_rounds
);
```

## Performance Optimizations

### Sampling Improvements
- Dynamic sample size adjustment based on available peers
- Fallback mechanisms for insufficient peer responses
- Configurable retry limits and timeouts

### Network Efficiency
- Heartbeat-based liveness detection
- Reduced unnecessary message traffic
- Better handling of network partitions

## Monitoring and Debugging

### Statistics Collection
```rust
let stats = node_logic.get_stats();
println!("Rounds executed: {}", stats.rounds_executed);
println!("Queries sent: {}", stats.queries_sent);
println!("Color changes: {}", stats.color_changes);
```

### State Inspection
```rust
let is_decided = node_logic.is_decided();
let current_candidate = node_logic.get_current_candidate();
```

## Testing

The implementation includes comprehensive tests covering:
- Parameter validation
- Message handling
- Consensus convergence
- Performance features

Run tests with:
```bash
cargo test --package simba --lib logic::snowball
```

## Configuration Files

### Protocol Configuration (`library/protocols/snowball.ron`)
```ron
Snowball(
    acceptance_threshold: 10,
    sample_size_weighted: 0.25,
    query_threshold_weighted: 0.6,
    heartbeat_interval: 1000,
    max_rounds: 1000,
)
```

### Experiment Configuration (`library/experiments/snowball.ron`)
```ron
(
    protocol: "snowball",
    network: "a2a_large",
    metrics: [Throughput, Latency, NetworkMessages],
    data_ranges: [
        (AcceptanceThreshold, LinearInt(start: 5, end: 50, step_size: 5)),
        (HeartbeatInterval, LinearInt(start: 500, end: 2000, step_size: 500)),
        (MaxRounds, LinearInt(start: 500, end: 2000, step_size: 500)),
    ],
    timeout: Blocks(warmup: 0, runtime: 1),
)
```

## Future Enhancements

### Planned Features
- **Transaction Support**: Handle actual blockchain transactions
- **Dynamic Parameter Adjustment**: Adaptive parameter tuning based on network conditions
- **Performance Profiling**: Detailed performance analysis tools
- **Network Topology Optimization**: Better support for various network structures

### Research Areas
- **Consensus Convergence**: Optimizing convergence speed
- **Network Efficiency**: Reducing message overhead
- **Fault Tolerance**: Improving Byzantine resilience
- **Scalability**: Supporting larger network sizes

## Contributing

When contributing to the Snowball implementation:

1. **Follow Rust Best Practices**: Use idiomatic Rust patterns
2. **Add Tests**: Include tests for new functionality
3. **Update Documentation**: Keep this README current
4. **Performance Considerations**: Consider impact on consensus speed

## References

- [Avalanche Consensus Paper](https://arxiv.org/abs/1906.08936)
- [Snowball Consensus Protocol](https://github.com/ava-labs/avalanchego)
- [Byzantine Fault Tolerance](https://en.wikipedia.org/wiki/Byzantine_fault_tolerance)
