//! Query benchmark suite for tinyOLAP.
//!
//! See `benches/README.md` for the rationale behind dataset shape and query
//! selection. This file is intentionally a single binary: dataset spec,
//! deterministic generator, and criterion harness all live together so the
//! whole thing is one `cargo bench --bench queries` invocation.
//!
//! On first run the generator builds the dataset under `benches/data/events/`
//! (gitignored). Subsequent runs reuse it. Delete that directory to regenerate.

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
    RecordBatch, StringArray,
};
use arrow::datatypes::{Field, Schema};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use tinyolap::catalog::schema::{ColumnSchema, DataType, TableSchema};
use tinyolap::run_select_collect;
use tinyolap::storage::table_writer::TableWriter;

// -----------------------------------------------------------------------------
// Dataset spec
// -----------------------------------------------------------------------------

/// Total rows across the whole table. Big enough to make CPU differences
/// observable (sub-second → multi-second per query) and small enough that the
/// suite finishes in a few minutes.
const TOTAL_ROWS: usize = 25_000_000;

/// Rows per `TableWriter::insert` call. One insert = one part. Multiple parts
/// matter for future parallel-scan work; 5 parts is a reasonable starting
/// shape without making compaction land on day one.
const ROWS_PER_PART: usize = 5_000_000;

/// Fixed seed so every machine sees the same dataset and runs are comparable.
const RNG_SEED: u64 = 42;

/// Dataset location, resolved relative to `CARGO_MANIFEST_DIR`.
const DATASET_SUBDIR: &str = "benches/data/events";

/// 20 distinct event types — gives Q8 (string GROUP BY) a small but realistic
/// cardinality. Paired with Q5 (~200 integer groups) the delta is the cost of
/// string hashing over integer hashing.
const EVENT_TYPES: &[&str] = &[
    "click", "view", "purchase", "scroll", "hover",
    "login", "logout", "search", "share", "bookmark",
    "add_to_cart", "remove_from_cart", "checkout", "refund", "subscribe",
    "unsubscribe", "play", "pause", "complete", "skip",
];

fn build_schema() -> TableSchema {
    use DataType::*;
    TableSchema {
        name: "events".into(),
        columns: vec![
            // 8 query-relevant columns
            ColumnSchema { name: "ts".into(),          data_type: I64 },
            ColumnSchema { name: "user_id".into(),     data_type: I64 },
            ColumnSchema { name: "country_id".into(),  data_type: I16 },
            ColumnSchema { name: "city_id".into(),     data_type: I32 },
            ColumnSchema { name: "device".into(),      data_type: I8  },
            ColumnSchema { name: "event_type".into(),  data_type: Str },
            ColumnSchema { name: "price".into(),       data_type: F64 },
            ColumnSchema { name: "quantity".into(),    data_type: I32 },
            ColumnSchema { name: "duration_ms".into(), data_type: I32 },
            // 4 padding columns — never queried, only widen rows so the
            //   single-column-scan win is measurable.
            ColumnSchema { name: "flag_a".into(),      data_type: Bool },
            ColumnSchema { name: "flag_b".into(),      data_type: Bool },
            ColumnSchema { name: "session_id".into(),  data_type: I64  },
            ColumnSchema { name: "event_code".into(),  data_type: I16  },
        ],
        // Sorted by `ts` — gives a realistic part layout for future
        // ZoneMap / SortedAggregate work.
        sort_key: vec![0],
    }
}

// -----------------------------------------------------------------------------
// Dataset generation
// -----------------------------------------------------------------------------

fn dataset_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be set by cargo");
    PathBuf::from(manifest).join(DATASET_SUBDIR)
}

/// Generate the dataset if absent; otherwise just open the existing schema.
/// Returns the schema the queries should plan against.
fn ensure_dataset(dir: &Path) -> TableSchema {
    if dir.join("schema.json").exists() {
        return TableSchema::open(dir).expect("failed to open existing dataset schema");
    }

    eprintln!(
        "Generating dataset: {} rows × {} columns into {:?} (one-time)…",
        TOTAL_ROWS,
        build_schema().columns.len(),
        dir,
    );
    std::fs::create_dir_all(dir).expect("failed to create dataset dir");
    let schema = build_schema();
    TableSchema::create(dir, &schema).expect("failed to write schema.json");

    let writer = TableWriter::open(dir.to_path_buf()).expect("failed to open TableWriter");

    let mut rng = SmallRng::seed_from_u64(RNG_SEED);
    let mut ts_cursor: i64 = 0;

    let num_parts = TOTAL_ROWS.div_ceil(ROWS_PER_PART);
    let mut rows_remaining = TOTAL_ROWS;
    let mut part_idx = 0usize;
    while rows_remaining > 0 {
        let chunk_rows = rows_remaining.min(ROWS_PER_PART);
        let batch = generate_batch(&schema, chunk_rows, &mut rng, &mut ts_cursor);
        writer.insert(batch).expect("insert failed during dataset gen");
        rows_remaining -= chunk_rows;
        part_idx += 1;
        eprintln!("  part {}/{} written ({} rows)", part_idx, num_parts, chunk_rows);
    }
    eprintln!("Dataset ready.");

    TableSchema::open(dir).expect("failed to re-open dataset schema")
}

fn generate_batch(
    schema: &TableSchema,
    n: usize,
    rng: &mut SmallRng,
    ts_cursor: &mut i64,
) -> RecordBatch {
    // ts: monotonically non-decreasing with small per-row jitter — sorted by
    // construction, so the part layout matches what a real time-series table
    // produced by an ingestion pipeline would look like.
    let ts: Vec<i64> = (0..n)
        .map(|_| {
            let v = *ts_cursor;
            *ts_cursor += 1 + rng.random_range(0i64..3);
            v
        })
        .collect();

    let user_id:     Vec<i64>  = (0..n).map(|_| rng.random_range(0i64..1_000_000)).collect();
    let country_id:  Vec<i16>  = (0..n).map(|_| rng.random_range(0i16..200)).collect();
    let city_id:     Vec<i32>  = (0..n).map(|_| rng.random_range(0i32..10_000)).collect();
    let device:      Vec<i8>   = (0..n).map(|_| rng.random_range(0i8..5)).collect();
    let event_type:  Vec<&str> = (0..n)
        .map(|_| EVENT_TYPES[rng.random_range(0usize..EVENT_TYPES.len())])
        .collect();
    let price:       Vec<f64>  = (0..n).map(|_| rng.random_range(0.0f64..1000.0)).collect();
    let quantity:    Vec<i32>  = (0..n).map(|_| rng.random_range(1i32..50)).collect();
    let duration_ms: Vec<i32>  = (0..n).map(|_| rng.random_range(0i32..600_000)).collect();
    let flag_a:      Vec<bool> = (0..n).map(|_| rng.random_range(0u8..2) == 1).collect();
    let flag_b:      Vec<bool> = (0..n).map(|_| rng.random_range(0u8..2) == 1).collect();
    let session_id:  Vec<i64>  = (0..n).map(|_| rng.random_range(0i64..i64::MAX)).collect();
    let event_code:  Vec<i16>  = (0..n).map(|_| rng.random_range(0i16..i16::MAX)).collect();

    let arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(ts)),
        Arc::new(Int64Array::from(user_id)),
        Arc::new(Int16Array::from(country_id)),
        Arc::new(Int32Array::from(city_id)),
        Arc::new(Int8Array::from(device)),
        Arc::new(StringArray::from(event_type)),
        Arc::new(Float64Array::from(price)),
        Arc::new(Int32Array::from(quantity)),
        Arc::new(Int32Array::from(duration_ms)),
        Arc::new(BooleanArray::from(flag_a)),
        Arc::new(BooleanArray::from(flag_b)),
        Arc::new(Int64Array::from(session_id)),
        Arc::new(Int16Array::from(event_code)),
    ];

    let fields: Vec<Field> = schema
        .columns
        .iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));
    RecordBatch::try_new(arrow_schema, arrays).expect("RecordBatch::try_new failed")
}

// -----------------------------------------------------------------------------
// Query suite
// -----------------------------------------------------------------------------

/// (bench_id, SQL). One row per query — see README for the design rationale.
const QUERIES: &[(&str, &str)] = &[
    // Q1: pure scan + SUM on one column. Best case for columnar storage.
    //     Baseline for "how fast can we sum 25M f64s end-to-end."
    ("Q1_scan_sum_single_column",
        "SELECT SUM(price) FROM events"),

    // Q2: five aggregates over the same scan. Tests per-batch overhead
    //     amortization — adding more accumulators shouldn't cost much
    //     beyond the per-row arithmetic.
    ("Q2_scan_many_aggregates",
        "SELECT SUM(price), AVG(quantity), MIN(duration_ms), MAX(duration_ms), COUNT(price) FROM events"),

    // Q3: low-selectivity filter (~0.5% rows pass) on the sort-aligned
    //     column `ts`. Today scans all 25M rows; future ZoneMaps should
    //     skip nearly every granule since `ts` is monotonic per-part.
    //     The clearest demo target for ZoneMap when that lands.
    //
    //     TYPE-COERCION WORKAROUND: we want `WHERE country_id = 5` here
    //     (low cardinality, ~0.5%), but integer literals always lower to
    //     i64 and the filter operator doesn't widen — Int16 == Int64
    //     fails in arrow. Using `ts` (i64) sidesteps the bug. Revert to
    //     `WHERE country_id = 5` once the TypeCoercion optimizer rule
    //     lands.
    ("Q3_filter_low_selectivity",
        "SELECT SUM(price) FROM events WHERE ts < 250000"),

    // Q4: high-selectivity filter (~50% rows pass) on `user_id` — a
    //     non-sort-aligned column. Future ZoneMaps will *not* help here
    //     (user_id is random, every granule's min/max spans the full
    //     range), so this measures filter operator cost without any
    //     scan-skip benefit. Pairs with Q3: same filter cost, different
    //     skip potential.
    //
    //     TYPE-COERCION WORKAROUND: we want `WHERE country_id < 100`
    //     here for the same reason as Q3. Using `user_id` (i64) avoids
    //     the i16-vs-i64 literal mismatch.
    ("Q4_filter_high_selectivity",
        "SELECT SUM(price) FROM events WHERE user_id < 500000"),

    // Q5: GROUP BY low-cardinality integer (~200 groups). Hash table
    //     fits comfortably in L2. Pure HashAggregate happy path.
    ("Q5_groupby_low_cardinality_int",
        "SELECT country_id, SUM(price) FROM events GROUP BY country_id"),

    // Q6: GROUP BY high-cardinality integer (~1M groups). Hash table
    //     blows out of cache; per-probe cost is dominated by memory
    //     latency. Future SortedAggregate (GROUP BY on sort prefix) will
    //     beat this in O(1) memory.
    ("Q6_groupby_high_cardinality_int",
        "SELECT user_id, SUM(price), COUNT(price) FROM events GROUP BY user_id"),

    // Q7: realistic mixed query — filter + multi-key GROUP BY + agg.
    //     Exercises operator-chain overhead between FilterExec and
    //     HashAggregateExec.
    ("Q7_filter_plus_multikey_groupby",
        "SELECT country_id, city_id, AVG(price) FROM events WHERE ts > 5000000 GROUP BY country_id, city_id"),

    // Q8: string GROUP BY (~20 groups). Delta vs Q5 (~200 int groups)
    //     isolates the cost of hashing/comparing variable-length keys.
    ("Q8_groupby_string",
        "SELECT event_type, SUM(price) FROM events GROUP BY event_type"),
];

fn bench_queries(c: &mut Criterion) {
    let dir = dataset_dir();
    let schema = ensure_dataset(&dir);

    let mut group = c.benchmark_group("queries");
    // Throughput = rows scanned (always the full table today; no granule
    //   skipping yet). Lets criterion print rows/sec alongside ms.
    group.throughput(Throughput::Elements(TOTAL_ROWS as u64));

    for (name, sql) in QUERIES {
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                let batches = run_select_collect(sql, &schema, &dir).expect("query failed");
                black_box(batches);
            });
        });
    }
    group.finish();
}

fn criterion_config() -> Criterion {
    // Default sample_size=100 × hundreds-of-ms-per-query = many minutes per
    // bench. 20 samples is enough for criterion's bootstrap to produce a
    // tight CI on these queries since run-to-run variance is low (fixed
    // dataset, warm cache after criterion's own warm-up). measurement_time
    // is generous (25s) so slow queries — Q6 (~1M groups) and Q7 — can
    // still fit 20 samples without criterion warning about an insufficient
    // budget.
    Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(60))
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = bench_queries
}
criterion_main!(benches);
