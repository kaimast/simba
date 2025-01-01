#! /bin/env python3

''' Plots statistics for a specific run '''

# pylint: disable=too-many-locals,too-many-statements,too-many-branches,fixme

import os
import sys
import argparse

from sys import stderr

import matplotlib.pyplot as plt
from seaborn import lineplot
from pandas import read_csv, concat

def _main():
    parser = argparse.ArgumentParser()
    parser.add_argument('path', type=str)
    parser.add_argument('--prefix', type=str,
             help='Only consider metrics with specific prefixes, e.g. "nodes"')
    parser.add_argument('--filter', type=str, help="Select which metrics to use")
    parser.add_argument('--font-size', type=int, default=4)
    parser.add_argument('--linewidth', type=float, default=0.5)
    parser.add_argument('--outfile', type=str, default='statistics.pdf')
    args = parser.parse_args()

    plt.rcParams.update({'font.size': args.font_size})

    metric_names = {
        "incoming_data": "Incoming Data",
        "network_traffic": "Network Traffic",
    }

    data = read_csv(args.path, header=0)

    # convert time to seconds
    data["time"] = data["time"] / 1000.0

    metrics = map(lambda a: a.split('.')[-1], list(data.columns.values))
    metrics = set(filter(lambda a: a != "time", metrics))
    num_metrics = len(metrics)

    cluster_cols = filter(lambda a: "network." in a, list(data.columns.values))
    network_metrics = set(map(lambda a: a.split('.')[-1], cluster_cols))
    print(f"Found network metrics: {network_metrics}")

    node_cols = list(filter(lambda a: "nodes." in a, list(data.columns.values)))
    nodes = set(map(lambda a: a.split('.')[1], node_cols))
    node_metrics = set(map(lambda a: a.split('.')[-1], node_cols))
    print(f"Found node metrics: {node_metrics}")

    if num_metrics == 0:
        stderr.write("ERROR: need at least one metric\n")
        sys.exit(1)

    _fig, axes = plt.subplots(num_metrics, 1)
    if num_metrics == 1:
        axes = [axes]

    node_data = None
    for node_idx in nodes:
        prefix = f"nodes.{node_idx}."
        column_names = [col for col in data.columns if col.startswith(prefix) or col == "time"]
        this_node = data[column_names].copy()
        this_node["node"] = f"#{node_idx}"
        this_node.columns = [col.replace(prefix, '') for col in this_node.columns]
        if node_data is None:
            node_data = this_node
        else:
            node_data = concat([node_data, this_node])

    for (metric_idx, metric_name) in enumerate(metrics):
        axis = axes[metric_idx]

        if metric_name in ["job-runtime", "total-job-runtime", "num-objects"]:
            # show runtime in log scale (first instance spawn takes long)
            axis.set_yscale("log")
        else:
            axis.set_yscale("linear")

        if metric_name in network_metrics:
            metric_key = "network."+metric_name
            plot_data = data
            lineplot(plot_data, x="time", y=metric_key,
                     linewidth=args.linewidth, ax=axis)
        elif metric_name in node_metrics:
            assert node_data is not None
            plot_data = node_data
            metric_key = metric_name
            lineplot(node_data, x="time", y=metric_name,
                     hue="node", linewidth=args.linewidth, ax=axis)
            axis.legend(title="Node")
        else:
            raise RuntimeError(f"Invalid state for metric {metric_name}")

        if metric_idx+1 == len(metrics):
            # only label x axis once
            axis.set_xlabel("Time (s)")
        else:
            xax = axis.get_xaxis()
            xax.set_visible(False)

        axis.set(ylim=(0, None))
        axis.set_ylabel(metric_names[metric_name])

    plt.tight_layout()

    print(f'Writing output to {args.outfile}')
    plt.savefig(args.outfile)

if __name__ == "__main__":
    _main()
