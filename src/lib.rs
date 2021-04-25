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

pub mod api_client;
pub mod database;
pub mod logger;
pub mod search;

use api_client::SolvedPuzzleStats;
use chrono::{naive::NaiveDate, Datelike, Duration, Weekday};
use database::Database;
use serde::{Deserialize, Serialize};
use std::cmp;

// Size of each block of dates to fetch metadata about. Currently hard-coded to match the expected
// server response with no validation that this is correct.
pub const DAY_STEP: i64 = 100;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize, Serialize)]
pub struct PuzzleStats {
    pub date: NaiveDate,
    /// id used to identify a puzzle to NYT server
    pub puzzle_id: Option<u32>,
    weekday: Weekday,
    // It would be nice to embed SolvedPuzzleStats here, but serde's flatten attribute doesn't play
    // well with the csv crate
    pub solve_time_secs: Option<u32>,
    opened_unix: Option<u32>,
    solved_unix: Option<u32>,
    pub cheated: Option<bool>,
}

impl PuzzleStats {
    #[must_use]
    pub fn new(date: NaiveDate, id: u32, solve_stats: Option<SolvedPuzzleStats>) -> Self {
        let weekday = date.weekday();
        Self {
            date,
            puzzle_id: Some(id),
            weekday,
            solve_time_secs: solve_stats.map(|s| s.solve_time),
            opened_unix: solve_stats.and_then(|s| s.opened),
            solved_unix: solve_stats.and_then(|s| s.solved),
            cheated: Some(false),
        }
    }

    #[must_use]
    pub fn empty(date: NaiveDate) -> Self {
        let weekday = date.weekday();
        Self {
            date,
            weekday,
            puzzle_id: None,
            solve_time_secs: None,
            opened_unix: None,
            solved_unix: None,
            cheated: Some(false),
        }
    }

    /// Returns true if there is no more information to fetch for the given record because it has
    /// already been completed, with or without cheats, and all expected fields are filled.
    pub fn is_complete(&self) -> bool {
        self.puzzle_id.is_some()
            && (self.solve_time_secs.is_some() || self.cheated.unwrap_or(false))
    }

    /// Update the given record with information from the given `SolvedPuzzleStats`
    pub fn update_stats(&mut self, stats: SolvedPuzzleStats) {
        if stats.cheated {
            self.cheated = Some(true);
            self.solve_time_secs = None;
        } else {
            self.cheated = Some(false);
            self.solve_time_secs = Some(stats.solve_time);
        }
        self.opened_unix = stats.opened;
        self.solved_unix = stats.solved;
    }
}

/// Get records within the given range, inclusive, that are missing ids, including for days that
/// are not present in the database. The results are split into chunks no more than
/// `max_chunk_duration` long for convenience, as the NYT id APIs allow batched lookup of ids.
#[must_use]
pub fn get_days_without_ids_chunked(
    database: &Database,
    start: NaiveDate,
    end: NaiveDate,
    max_chunk_duration: Duration,
) -> Vec<Vec<PuzzleStats>> {
    let mut chunks: Vec<Vec<PuzzleStats>> = Vec::new();
    let mut current_start = start;
    while current_start <= end {
        // Find next date in given range on or after current_start that does not have a cached id
        // in the database
        current_start = match current_start
            .iter_days()
            .take_while(|date| *date <= end)
            .find(|date| match database.get(*date) {
                Some(record) => record.puzzle_id.is_none(),
                None => true,
            }) {
            Some(date) => date,
            None => break,
        };
        let current_end = cmp::min(end, current_start + max_chunk_duration - Duration::days(1));
        // Create a block of stats records that are missing ids
        let block: Vec<PuzzleStats> = current_start
            .iter_days()
            .take_while(|date| *date <= current_end)
            .filter_map(|date| {
                if let Some(record) = database.get(date) {
                    if record.puzzle_id.is_none() {
                        Some(record)
                    } else {
                        None
                    }
                } else {
                    // The date does not exist in the database at all
                    Some(PuzzleStats::empty(date))
                }
            })
            .collect();
        chunks.push(block);
        current_start = current_end + Duration::days(1);
    }

    chunks
}

/// Get records from database that have a cached puzzle id but aren't known to be solved
#[must_use]
pub fn get_cached_unsolved_records(database: &Database, start: NaiveDate) -> Vec<PuzzleStats> {
    let mut records = database.records();
    records.retain(|r| !r.is_complete() && r.puzzle_id.is_some() && r.date >= start);
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use api_client::SolvedPuzzleStats;
    use std::default::Default;
    use tempfile::NamedTempFile;

    #[test]
    /// Test get_days_without_ids_chunked
    /// TODO: add more test coverage
    fn days_without_ids() -> Result<()> {
        fn contains_date(haystack: &Vec<Vec<PuzzleStats>>, date: NaiveDate) -> bool {
            haystack
                .into_iter()
                .flatten()
                .any(|record| record.date == date)
        }

        let file = NamedTempFile::new()?;
        let path = file.into_temp_path().to_path_buf();
        let mut db = Database::new(path);
        // Empty record
        let empty_date = NaiveDate::from_ymd(2020, 1, 1);
        db.add(PuzzleStats::empty(empty_date));
        // Record with solve stats but without an id
        let solved_no_id_date = NaiveDate::from_ymd(2020, 1, 2);
        let mut solved_no_id =
            PuzzleStats::new(solved_no_id_date, 0, Some(SolvedPuzzleStats::default()));
        solved_no_id.puzzle_id = None;
        db.add(solved_no_id);
        // Record with solve stats and id
        let solved_ided_date = NaiveDate::from_ymd(2020, 1, 3);
        db.add(PuzzleStats::new(
            solved_ided_date,
            20,
            Some(SolvedPuzzleStats::default()),
        ));
        // Record with no solve stats but with an id
        let unsolved_ided_date = NaiveDate::from_ymd(2020, 1, 4);
        db.add(PuzzleStats::new(unsolved_ided_date, 100, None));
        // Record with cheated solve and with an id
        let cheated_ided_date = NaiveDate::from_ymd(2020, 1, 8);
        db.add(PuzzleStats::new(
            cheated_ided_date,
            400,
            Some(SolvedPuzzleStats {
                cheated: true,
                ..Default::default()
            }),
        ));
        // Record with cheated solve and no id
        let cheated_unided_date = NaiveDate::from_ymd(2020, 1, 9);
        let mut cheated_unided = PuzzleStats::new(
            cheated_unided_date,
            0,
            Some(SolvedPuzzleStats {
                cheated: true,
                ..Default::default()
            }),
        );
        cheated_unided.puzzle_id = None;
        db.add(cheated_unided);

        let start = NaiveDate::from_ymd(2020, 1, 1);
        let end = NaiveDate::from_ymd(2020, 1, 11);

        let chunks = get_days_without_ids_chunked(&db, start, end, Duration::days(5));
        assert!(
            contains_date(&chunks, empty_date),
            "Empty record should be returned"
        );
        assert!(
            contains_date(&chunks, solved_no_id_date),
            "Solved, no-id record should be returned"
        );
        assert!(
            !contains_date(&chunks, solved_ided_date),
            "Solved, ided record should not be returned"
        );
        assert!(
            !contains_date(&chunks, unsolved_ided_date),
            "Unsolved, ided record should not be returned"
        );
        assert!(
            !contains_date(&chunks, cheated_ided_date),
            "Cheated, ided record should not be returned"
        );
        assert!(
            contains_date(&chunks, cheated_unided_date),
            "Cheated, unided record should not be returned"
        );
        assert!(
            contains_date(&chunks, end),
            "Ensure end date is returned (if empty)"
        );
        assert!(
            !contains_date(&chunks, end + Duration::days(1)),
            "The end date should be the last included date"
        );

        Ok(())
    }

    #[test]
    /// Test get_days_without_ids_chunked
    /// TODO: add more test coverage
    fn test_get_cached_unsolved_records() -> Result<()> {
        fn contains_date(haystack: &Vec<PuzzleStats>, date: NaiveDate) -> bool {
            haystack.into_iter().any(|record| record.date == date)
        }

        let file = NamedTempFile::new()?;
        let path = file.into_temp_path().to_path_buf();
        let mut db = Database::new(path);
        // Empty record
        let empty_date = NaiveDate::from_ymd(2020, 1, 1);
        db.add(PuzzleStats::empty(empty_date));
        // Record with solve stats but without an id
        let solved_no_id_date = NaiveDate::from_ymd(2020, 1, 2);
        let mut solved_no_id =
            PuzzleStats::new(solved_no_id_date, 0, Some(SolvedPuzzleStats::default()));
        solved_no_id.puzzle_id = None;
        db.add(solved_no_id);
        // Record with solve stats and id
        let solved_ided_date = NaiveDate::from_ymd(2020, 1, 3);
        db.add(PuzzleStats::new(
            solved_ided_date,
            20,
            Some(SolvedPuzzleStats::default()),
        ));
        // Record with no solve stats but with an id
        let unsolved_ided_date = NaiveDate::from_ymd(2020, 1, 4);
        db.add(PuzzleStats::new(unsolved_ided_date, 100, None));
        // Record with cheated solve and with an id
        let cheated_ided_date = NaiveDate::from_ymd(2020, 1, 8);
        db.add(PuzzleStats::new(
            cheated_ided_date,
            400,
            Some(SolvedPuzzleStats {
                cheated: true,
                ..Default::default()
            }),
        ));
        // Record with cheated solve and no id
        let cheated_unided_date = NaiveDate::from_ymd(2020, 1, 9);
        let mut cheated_unided = PuzzleStats::new(
            cheated_unided_date,
            0,
            Some(SolvedPuzzleStats {
                cheated: true,
                ..Default::default()
            }),
        );
        cheated_unided.puzzle_id = None;
        db.add(cheated_unided);

        assert!(get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 5)).is_empty());
        assert!(get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 8)).is_empty());
        assert!(get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 9)).is_empty());
        assert!(get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 10)).is_empty());

        let cached_unsolved = get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 4));
        assert!(cached_unsolved.len() == 1);
        assert!(contains_date(&cached_unsolved, unsolved_ided_date));

        let cached_unsolved = get_cached_unsolved_records(&db, NaiveDate::from_ymd(2020, 1, 1));
        assert!(cached_unsolved.len() == 1);
        assert!(contains_date(&cached_unsolved, unsolved_ided_date));

        Ok(())
    }
}
