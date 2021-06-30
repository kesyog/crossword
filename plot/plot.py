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
"""
Generates a plot of crossword statistics generated by the crossword crate

It expects two positional arguments:
1. The path to a CSV file generated from the crossword crate
2. The output path and filename where the rendered plot should be saved. Both SVG and PNG formats
are supported.
"""

import datetime
import numpy as np
import seaborn as sns
import matplotlib.pyplot as plt
import pandas as pd
import os
import sys

plt.style.use('Solarize_Light2')
DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']


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


def save_plot(df, out_path):
    # Pick an appropriate y-axis, balancing being robust to outliers vs. showing all data
    Y_MAX = df["solve_time_secs"].quantile(0.99) / 60
    Y_MAX = 60 # unfortunately, an hour is a good upper limit for me.

    fig = plt.figure(figsize=(10, 7), dpi=200)
    today = datetime.date.today().isoformat()
    plt.title(
        f"NYT crossword solve time (8-week rolling average) as of {today}"
    )
    ax = fig.gca()
    for day in DAYS:
        rolling_avg = df[df["weekday"] == day]["solve_time_secs"].rolling("56D").mean()
        (rolling_avg / 60.0).plot(
            ax=ax, label=day, linewidth=2, markersize=4, marker="o", linestyle="-"
        )
    plt.legend()

    ax.set_xlabel("Solve Date")
    ax.set_ylabel("Minutes")
    minor_yticks = np.arange(0, Y_MAX + 1, 5)
    ax.set_ylim(0, Y_MAX)
    ax.set_yticks(minor_yticks, minor=True)

    plt.xticks(rotation=0)

    plt.grid(True, which="both", axis="both")
    plt.savefig(out_path)


def save_vln_plot(df, out_path):
    # ridge plot may be fun to try too:
    # https://seaborn.pydata.org/examples/kde_ridgeplot.html
    df['solve_time_m'] = df['solve_time_secs'] / 60.0
    ax = sns.violinplot(x="weekday", y="solve_time_m", data=df, order=DAYS)

    date = max(df['Solved datetime']).strftime("%b %d, %Y")
    ax.set_title("%d NYT Crossword Solve Times by Day of Week as of %s" % (len(df), date))
    ax.set_xlabel("Day of Week")
    ax.set_ylabel("Minutes to Solve")

    ax.set_ylim(0, 65)
    ax.set_yticks(np.arange(0, 60, 5))

    ax.get_legend().remove()
    plt.savefig(out_path)
    plt.close()


def save_split_vln_plot(df, out_path):
    # minor tweak to save_vln_plot, too much repitition...
    df['solve_time_m'] = df['solve_time_secs'] / 60.0
    df['In 2021'] = df['Solved datetime'] > datetime.datetime(2021, 1, 1)
    ax = sns.violinplot(x="weekday", y="solve_time_m", hue='In 2021',
        split=True, data=df, bw=.25, order=DAYS)

    date = max(df['Solved datetime']).strftime("%b %d, %Y")
    ax.set_title("%d NYT Crossword Solve Times by Day of Week as of %s" % (len(df), date))
    ax.set_xlabel("Day of Week")
    ax.set_ylabel("Minutes to Solve")

    ax.set_ylim(0, 65)
    ax.set_yticks(np.arange(0, 65, 5))

    ax.legend() # seems to have the effect of removing the title of the legend?
    handles, labels  = ax.get_legend_handles_labels()
    ax.legend(handles, ["Before 2021", "2021"], loc="upper left")

    plt.savefig(out_path)
    plt.close()


def generate(in_file, out_file):
    # sns.set_style("ticks")
    df = parse_data(in_file)
    save_plot(df, out_file)
    save_vln_plot(df, "%s Violin Plot%s" % os.path.splitext(out_file))
    save_split_vln_plot(df, "%s Split Violin Plot%s" % os.path.splitext(out_file))


def main():
    try:
        in_file = sys.argv[1]
        out_file = sys.argv[2]
    except:
        print(
            "Required arguments not given. Usage: {} <input_csv_file> <output_file>".format(
                sys.argv[0]
            )
        )
        return

    generate(in_file, out_file)


if __name__ == "__main__":
    main()