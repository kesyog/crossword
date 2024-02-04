# Scraping NYT crossword stats ⬛⬜⬜⬜⬜⬛

[![build status](https://img.shields.io/github/actions/workflow/status/kesyog/crossword/push.yml?branch=main&style=flat-square)](https://github.com/kesyog/crossword/actions/workflows/push.yml)
[![Apache 2.0 license](https://img.shields.io/github/license/kesyog/crossword?style=flat-square)](./LICENSE)

When I first subscribed to the _New York Times_ crossword in January 2017, I could solve some Monday
puzzles and maybe the occasional Tuesday. Over time, I slowly got better at noticing the common
patterns and picked up enough [crosswordese](https://en.wikipedia.org/wiki/Crosswordese) to be able
to finish it fairly consistently. As of February 2024, I've now maintained a daily streak stretching
back 6+ years to August 2017 (knock on wood...).

The NYT crossword app exposes some solve time statistics like all-time average and fastest time,
both broken out day. But there is no good way to see how much you have improved over time. You can
open each puzzle in the archive individually and see your times, but there aren't any overview plots
or officially-sanctioned developer APIs to fetch your data. Luckily, all puzzle stats are fetched
via client-side Javascript, making it easy enough to scrape the data.

Regularly-updating plot:

![chart of solve times](https://storage.googleapis.com/xword-plots/plot.svg)

Some observations:

* The crossword clues _generally_ get harder over the course of the week, peaking in difficulty on
Saturday, and the correlation with solve times shows up pretty clearly in the solve times.
* The clues of Sunday puzzles are roughly at a Thursday or Friday difficulty level, but the grid is
extra-large, so it's usually the slowest day.
* Thursdays and Sundays usually have some sort of theme and/or trick, which usually added extra
difficulty, especially early on when I wasn't as familiar with the usual patterns that constructors
follow.

Caveats:

* I didn't count any puzzles that I didn't finish or that I used the "check" or "reveal" assists on,
so there's some survivorship bias. Again, this only affects the early data, as I've since stopped
using those features.
* I generally solve puzzles on my phone, but every now and then I'll solve them on my computer,
which shaves some time off. This is just another source of noise.

## Scraping the data

This repo contains a Rust crate that scrapes data from the NYT's servers. To use it, install [Rust](https://rustup.rs)
and then run the following to generate a CSV file containing the data:

```sh
# See help screen
$ cargo run --release -- --help

# Example usage starting search from the crossword on January 1, 2016 onward
# Subsequent program runs will use existing file as a cache 
$ cargo run --release -- -t <your NYT token> -s 2016-01-01 data.csv

# Example usage with increased quota to set rate-limit to 10 requests/second
$ cargo run --release -- -t <your NYT token> -s 2016-01-01 -q 10 -o data.csv
```

The NYT subscription token must be extracted via your browser (see below).

The program will fetch results concurrently, but by default, requests are limited to 5 per second to
reduce the load on NYT's servers. While you can choose to override that limit to speed up the
search, be nice and use something reasonable. There shouldn't be any need to run this script very
often so it's better to just err on the side of being slow. Regardless of the setting you use,
default or not, I'm not responsible for anything that happens to your account.

### Design goals

1. Reduce requests made to NYT's servers. If provided, data from previous runs is loaded and used to
avoid re-requesting information pulled from previous runs.
1. Reduce load on NYT's servers. Requests made to the server are rate-limited.
1. Maximum concurrency. Requests are made as concurrently as possible given the other two
constraints thanks to async/await. It's totally overkill with the default amount of rate-limiting 🤷🏽‍♂

### Extracting your subscription token

These instructions are for Google Chrome, but you should be able to do the equivalent with other
browsers' developer tools.

1. Open the Developer Tools dialog
1. Open the Network tab
1. Navigate to <https://www.nytimes.com/crosswords>
1. Look for a request for some kind of json file e.g. `progress.json`, `mini-stats.json`, or
`stats-and-streaks.json`.
1. In the headers pane, find the value of `nyt-s` in the list of request headers. That is your
token. If you can't find the `nyt-s` header in the request, try a different json file.

### Under the hood

The script scrapes data using some undocumented but public REST APIs used by the official NYT
crossword webpage.
The APIs can be found by using the included set of developer tools in Chrome or Firefox to peek at
HTTP traffic while browsing the crossword webpage.

Some details if you want to bypass the script and replicate the functionality yourself:

1. Each puzzle is assigned a numerical id. Before we can fetch the stats for a given puzzle, we need
to know that id. To find it, send a GET request as below, specifying `{start_date}` and `{end_date}`
in YYYY-MM-DD format ([ISO 8601](https://xkcd.com/1179)). The server response is limited to 100
puzzles and can be limited further by adding a `limit` parameter.

    ```sh
    curl 'https://www.nytimes.com/svc/crosswords/v3/36569100/puzzles.json?publish_type=daily&date_start={start_date}&date_end={end_date}' -H 'accept: application/json'
    ```

1. To fetch solve stats for a given puzzle, send a GET request as below, replacing `{id}` with the
puzzle id. This API requires a NYT crossword subscription. `{subscription_header}` can be found by
snooping on outgoing HTTP requests via Chrome/Firefox developer tools while opening a NYT crossword
in your browser. Alternatively, you can supposedly extract your session cookie from your browser and
send that instead (see linked reddit post below), but I haven't tried it myself.
  
    ```sh
    curl 'https://www.nytimes.com/svc/crosswords/v6/game/{id}.json' -H 'accept: application/json' -H 'nyt-s: {subscription_header}'
    ```

1. Check out the `calcs` and `firsts` field of this response to get information like solve duration,
when the puzzle was solved, and whether any assists were used.

1. Rinse and repeat, collecting data for the dates of interest.

## Plotting the data

Use your favorite tools to analyze and plot the raw data stored in the CSV file. The
Python-pandas-matplotlib trifecta works great.

My plots are generated via the Python script in the [plot](./plot) folder. To use it, run the following:

```sh
# Install prerequisites
$ pip3 install -r plot/requirements.txt

# Generate plot
$ plot/plot.py <path to csv file> <path to save plot image>
```

The output path can be an SVG or PNG file.

## Auto-updating

The plot above is auto-generated by a regularly-scheduled job running on the Google Cloud Platform.

[cloud\_run.py](./cloud_run.py) implements a Flask server that glues together the stats fetching and
plotting scripts, and the whole thing is containerized and run via Google Cloud Run.

## References

* [Relevant Reddit post][1]: for figuring out how to find the right APIs to hit
* [Rex Parker does the NY Times crossword][2]: grumpy old man

## Disclaimer

This is not an officially supported Google product

[1]: https://www.reddit.com/r/crossword/comments/dqtnca/my_automatic_nyt_crossword_downloading_script
[2]: https://rexwordpuzzle.blogspot.com
