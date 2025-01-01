#! /bin/env python3

import sys
import argparse
import seaborn as sns

from matplotlib import pyplot as plt
from pandas import read_csv

def _main():
    parser = argparse.ArgumentParser()
    parser.add_argument('filename', type=str)
    parser.add_argument('--x-axis', type=str, required=True)
    parser.add_argument('--y-axis', type=str, required=True)
    parser.add_argument('--outfile', type=str)

    args = parser.parse_args()

    df = read_csv(args.filename)

    if args.x_axis not in df:
        print(f"No such column {args.x_axis}. Options are: {df.columns}")
        sys.exit(-1)

    if args.y_axis not in df:
        print(f"No such column {args.y_axis}. Options are: {df.columns}")
        sys.exit(-1)

    sns.lineplot(x=args.x_axis, y=args.y_axis, data=df)

    if args.outfile:
        name = args.outfile
    else:
        name = args.filename.replace('.csv', '.pdf')

    print(f"Storing plot as '{name}'")
    plt.savefig(name)

if __name__ == "__main__":
    _main()
