# `webcrawler`

A library for enabling breadth-first crawling starting from a root URL.

## Features

* Dynamically set maximum concurrent tasks
* Dynamically set maximum number of pages to visit
* Skipping duplicate pages

## Command-line application

The library is used to expose a command-line program (`crawler-cli`) that
exposes its functionality according to the following API

```
$ cargo run -- --help

A command-line application that launches a crawler starting from a root url, and
descending to nested urls in a breadth-first manner

Usage: crawler-cli [OPTIONS] <ROOT_URL>

Arguments:
  <ROOT_URL>  The root url to start the crawling from

Options:
      --max-tasks <MAX_TASKS>  Max number of concurrent tasks to trigger [default: 5]
      --max-pages <MAX_PAGES>  Max number of pages to visit [default: 100]
  -h, --help                   Print help information
  -V, --version                Print version information
```
