// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::api_client::RateLimitedClient;
use crate::database::PuzzleStats;
use crate::logger;
use anyhow::Result;
use futures::future;
use log::{debug, error, warn};
use std::convert::TryInto;
use tokio::sync::mpsc;

pub async fn fetch_missing_times(
    client: RateLimitedClient,
    dates: Vec<PuzzleStats>,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    let mut futures = Vec::new();
    for puzzle in dates {
        futures.push(tokio::spawn(get_solve_stats(
            client.clone(),
            puzzle,
            logger.clone(),
        )));
    }
    future::join_all(futures).await;
    Ok(())
}

/// Concurrently fetch statistics for the crosswords from the given dates and send the results to
/// the provided channel
///
/// # Arguments
///
/// * `client` - A `RateLimitedClient` that can be used to send outgoing requests
/// * `dates` - Blocks of dates to search. Each block must be sorted and contain no more than
/// `DAY_STEP` elements
/// * `logger` - Channel where individual puzzle's statistics should be sent to
pub async fn fetch_ids_and_stats(
    client: RateLimitedClient,
    dates: Vec<Vec<PuzzleStats>>,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    let mut futures = Vec::new();
    for block_of_dates in dates {
        futures.push(tokio::spawn({
            let client = client.clone();
            let tx = logger.clone();
            async move { search_date_block(client, block_of_dates, tx).await }
        }));
    }

    future::join_all(futures).await;
    Ok(())
}

/// Concurrently search crosswords within the provided block of dates and send the results to the
/// provided channel
///
/// # Arguments
///
/// * `client` - A `RateLimitedClient` that can be used to send outgoing requests
/// * `block_of_dates` - Sorted list of puzzle dates to search. Must contain no more than
/// `DAY_STEP` elements
/// * `logger` - Channel where individual puzzle's statistics should be sent to
async fn search_date_block(
    client: RateLimitedClient,
    block: Vec<PuzzleStats>,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    assert!(block.len() <= crate::DAY_STEP.try_into().unwrap());
    let start = block[0].date;
    let end = block.iter().last().unwrap().date;

    debug!("Fetching ids for date range {} to {}", start, end);
    let id_map = match client.get_puzzle_ids(start, end).await {
        Ok(map) => map,
        Err(e) => {
            // This may occur if the entire date block consists of unreleased puzzles, which would
            // happen if the puzzle from the last date in the search block (today in UTC) hasn't
            // been released yet.
            warn!(
                "Couldn't get puzzle id for date range {} to {}. Error: {:?}",
                start, end, e
            );
            return Ok(());
        }
    };

    // Concurrently find stats for all puzzles in block
    let mut futures = Vec::new();
    for mut puzzle in block {
        let date = puzzle.date;
        puzzle.puzzle_id = if let Some(id) = id_map.get(&date) {
            Some(*id)
        } else {
            // This will occur if there are unreleased puzzles in this date block
            warn!("No id found for {}", date);
            logger.send(logger::Payload::FetchError(None))?;
            continue;
        };
        // Check if the solve time is already known. This would happen if the loaded database
        // contained a puzzle record that had a solve time but no saved id
        if puzzle.solve_time_secs.is_some() {
            logger.send(logger::Payload::Solve(puzzle)).unwrap();
            continue;
        }
        futures.push(tokio::spawn(get_solve_stats(
            client.clone(),
            puzzle,
            logger.clone(),
        )));
    }
    future::join_all(futures).await;
    Ok(())
}

async fn get_solve_stats(
    client: RateLimitedClient,
    mut puzzle: PuzzleStats,
    logger: mpsc::UnboundedSender<logger::Payload>,
) -> Result<()> {
    let id = puzzle.puzzle_id.unwrap();
    match client.get_solve_stats(id).await {
        Ok(Some(solve_stats)) => {
            puzzle.update_stats(solve_stats);
            logger.send(logger::Payload::Solve(puzzle)).unwrap();
        }
        Ok(None) => {
            logger.send(logger::Payload::Unsolved(puzzle)).unwrap();
        }
        Err(e) => {
            error!(
                "Failed to get stats for date={} id={}: {}",
                puzzle.date, id, e
            );
            // Send puzzle stats to get added to database anyway. At least we know its id.
            logger
                .send(logger::Payload::FetchError(Some(puzzle)))
                .unwrap();
        }
    }
    Ok(())
}
