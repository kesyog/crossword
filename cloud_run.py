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
HTTP server intended to be containerized, deployed to Google Cloud Run, and scheduled to run
regularly.

At each invocation:
1. The crossword database (CSV file) is fetched from Cloud Storage
2. The database is refreshed using a Rust program to pull data from NYT
3. The changes are pushed to Cloud Storage
4. A plot is generated and pushed to a Cloud Storage
"""

import os
import subprocess
from flask import Flask
from google.cloud import storage
import plot.plot as plot

app = Flask(__name__)

# Filename for stats CSV file in Cloud Storage bucket
CLOUD_CSV_FILENAME = "data.csv"
# Filename for stats CSV file on local fileystem
LOCAL_CSV_FILENAME = "data.csv"
# Filename for output plot in Cloud Storage bucket
CLOUD_PLOT_FILENAME = "plot.svg"
# Filename for output plot on local fileystem
LOCAL_PLOT_FILENAME = "plot.svg"


def download_csv(bucket):
    blob = bucket.blob(CLOUD_CSV_FILENAME)
    blob.download_to_filename(LOCAL_CSV_FILENAME)


def upload_csv(bucket):
    blob = bucket.blob(CLOUD_CSV_FILENAME)
    blob.upload_from_filename(LOCAL_CSV_FILENAME)


def upload_plot(bucket):
    blob = bucket.blob(CLOUD_PLOT_FILENAME)
    blob.upload_from_filename(LOCAL_PLOT_FILENAME)


def generate_plot():
    plot.generate(LOCAL_CSV_FILENAME, LOCAL_PLOT_FILENAME)


def update_csv():
    subprocess.run(
        ["crossword", LOCAL_CSV_FILENAME],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


@app.route("/")
def update_database_and_plot():
    storage_client = storage.Client()
    db_bucket_name = os.environ["DB_BUCKET_NAME"]
    db_bucket = storage_client.bucket(db_bucket_name)

    download_csv(db_bucket)
    update_csv()
    upload_csv(db_bucket)
    generate_plot()

    plot_bucket_name = os.environ["PLOT_BUCKET_NAME"]
    plot_bucket = storage_client.bucket(plot_bucket_name)
    upload_plot(plot_bucket)

    return "Success!"


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=int(os.environ.get("PORT", 8080)))
