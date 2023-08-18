#!/usr/bin/env python

# Copyright 2021 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
import argparse
import datetime
import matplotlib.pyplot as plt
import numpy as np
import os
import pandas as pd
import seaborn as sns


DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

A = argparse.ArgumentParser(
    description='Generates plots of crossword statistics from a CSV',
    formatter_class=argparse.ArgumentDefaultsHelpFormatter
)
A.add_argument('in_file', action='store', type=str,
    help="The path to a CSV file generated from the crossword crate")
A.add_argument('out_file', action='store', type=str,
    help="The output path where plots should be saved (e.g. trends.png)")
A.add_argument('-c', '--ceiling', action='store', type=int, default=0,
    help='Max Y Value in minutes; use 0 to compute from data.')
A.add_argument('-s', '--style', action='store', default="Solarize_Light2",
    type=str,  # choices=plt.style.available,  # looks gross
    help='Name of the plot style to use; must be in plt.style.available')


def parse_data(csv_path):
    """Parse crossword database stored at the given path into a pandas DataFrame. The DataFrame
    only contains solve data for unaided, solved puzzles and is sorted by the index, the time when
    each puzzle was solved.

    Interesting columns in the returned DataFrame:
    solve_time_secs
    weekday
    """
    df = pd.read_csv(csv_path, parse_dates=["date"], index_col="date")
    df["Solved datetime"] = pd.to_datetime(df["solved_unix"], unit="s")
    # Use the date solved rather than the puzzle date as the index.
    # Puzzle date is interesting for analyzing puzzle difficulty over time (but skewed by change
    # in ability over time)
    # Date solved is interesting for analyzing change in solving ability over time (assuming puzzle
    # difficulty is a constant)
    df.index = df["Solved datetime"]
    df = df.sort_index()
    # Filter out:
    # * Puzzles that were solved more than 7 days after first open. These puzzles were revisited
    # much later, making it hard to make accurate conclusions about the solve time.
    # * Unsolved puzzles
    # * Puzzles where cheats were used
    df = df[
        (df["solved_unix"] - df["opened_unix"] < 3600 * 24 * 7)
        & (df["cheated"] == False)
        & df.solve_time_secs.notnull()
    ]

    return df


def save_plot(df, out_path, ymax):
    fig = plt.figure(figsize=(10, 7), dpi=200)
    today = datetime.date.today().isoformat()
    plt.title(
        f"NYT crossword solve time (8-week rolling average) as of {today}"
    )
    ax = fig.gca()
    for day in DAYS:
        rolling_avg = df[df["weekday"] == day]["solve_time_secs"].rolling("56D").mean()
        (rolling_avg / 60.0).plot(
            ax=ax, label=day, linewidth=2, markersize=20, marker=",", linestyle="-"
        )
    plt.legend()

    ax.set_xlabel("Solve Date")
    ax.set_ylabel("Minutes")
    minor_yticks = np.arange(0, ymax + 1, 5)
    ax.set_ylim(0, ymax)
    ax.set_yticks(minor_yticks, minor=True)

    plt.xticks(rotation=0)

    plt.grid(True, which="both", axis="both")
    plt.savefig(out_path)


def save_vln_plot(df, out_path, ymax):
    """
        Makes a violin plot of the daily time distributions.

        df: dataframe containing crossword times
        out_path: filename to save plot to
        ceiling: max y-value to show
    """
    df['solve_time_m'] = df['solve_time_secs'] / 60.0
    ax = sns.violinplot(x="weekday", y="solve_time_m", data=df, order=DAYS)

    date = max(df['Solved datetime']).strftime("%b %d, %Y")
    ax.set_title("%d NYT Crossword Solve Times by Day of Week as of %s" % (len(df), date))
    ax.set_xlabel("Day of Week")
    ax.set_ylabel("Minutes to Solve")

    ax.set_ylim(0, ymax + 5)
    ax.set_yticks(np.arange(0, ymax, 5))

    ax.get_legend().remove()
    plt.savefig(out_path)
    plt.close()


def save_split_vln_plot(df, out_path, ymax):
    """
        Splits the violin plot into pre-2021 and 2021+ sections to look
        at progress over time.

        df: dataframe containing crossword times
        out_path: filename to save plot to
        ceiling: max y-value to show
    """
    df['solve_time_m'] = df['solve_time_secs'] / 60.0
    # TODO: should probably not hard-code 2021 and instead pass in a date.
    df['In 2021'] = df['Solved datetime'] > datetime.datetime(2021, 1, 1)
    ax = sns.violinplot(x="weekday", y="solve_time_m", hue='In 2021',
                        split=True, data=df, bw=.25, order=DAYS)

    date = max(df['Solved datetime']).strftime("%b %d, %Y")
    ax.set_title("%d NYT Crossword Solve Times by Day of Week as of %s" % (len(df), date))
    ax.set_xlabel("Day of Week")
    ax.set_ylabel("Minutes to Solve")

    ax.set_ylim(0, ymax + 5)
    ax.set_yticks(np.arange(0, ymax, 5))

    ax.legend()  # Seems to have the effect of removing the title of the legend
    handles, labels = ax.get_legend_handles_labels()
    ax.legend(handles, ["Before 2021", "2021"], loc="upper left")

    plt.savefig(out_path)
    plt.close()


def generate(in_file, out_file, ceiling):
    df = parse_data(in_file)

    if ceiling == 0:
        # Pick an appropriate y-axis, balancing being robust to outliers vs. showing all data
        ymax = df["solve_time_secs"].quantile(0.99) / 60
    else:
        ymax = ceiling

    save_plot(df, out_file, ymax)

    out_name = "%s Violin Plot%s" % os.path.splitext(out_file)
    save_vln_plot(df, out_name, ymax)

    out_name = "%s Split Violin Plot%s" % os.path.splitext(out_file)
    save_split_vln_plot(df, out_name, ymax)

    # TODO: a ridge plot may be fun to try too:
    # https://seaborn.pydata.org/examples/kde_ridgeplot.html


def main():
    args = A.parse_args()

    plt.style.use(args.style)
    generate(args.in_file, args.out_file, args.ceiling)


if __name__ == "__main__":
    main()
