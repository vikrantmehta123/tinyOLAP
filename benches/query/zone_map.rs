use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::Path;
use tempfile::tempdir;
use tinyolap::parser::ast::{CmpOp, Literal, Predicate};
use tinyolap::processors::{
    filter::Filter, full_scan::FullScan, processor::Processor, zone_map_scan::ZoneMapScan,
};
use tinyolap::storage::column_chunk::ColumnChunk;
use tinyolap::storage::schema::{ColumnDef, DataType, TableDef};
use tinyolap::storage::table_writer::TableWriter;

// Many small parts with disjoint ts ranges — part p covers
// ts in [p*PART_ROWS, (p+1)*PART_ROWS). Pruning only has something
// to do when there are multiple parts to skip.
const NUM_PARTS: usize = 100;
const PART_ROWS: usize = 10_000;
const TOTAL_ROWS: usize = NUM_PARTS * PART_ROWS;

fn schema() -> TableDef {
    TableDef {
        name: "events".to_string(),
        columns: vec![
            ColumnDef { name: "ts".to_string(),  data_type: DataType::I64 },
            ColumnDef { name: "val".to_string(), data_type: DataType::F64 },
        ],
        sort_key: vec![0],
    }
}

fn write_parts(dir: &Path) {
    let schema = schema();
    TableDef::create(dir, &schema).unwrap();
    let writer = TableWriter::open(dir.to_path_buf()).unwrap();

    for p in 0..NUM_PARTS {
        let base = (p * PART_ROWS) as i64;
        let ts: Vec<i64>  = (0..PART_ROWS as i64).map(|i| base + i).collect();
        let val: Vec<f64> = (0..PART_ROWS).map(|i| i as f64).collect();
        writer
            .insert(vec![ColumnChunk::I64(ts), ColumnChunk::F64(val)])
            .unwrap();
    }
}

/// Pull every batch through and count rows — keeps the work observable.
fn drain(mut node: Box<dyn Processor>) -> usize {
    let mut rows = 0;
    while let Some(batch) = node.next_batch() {
        let batch = batch.unwrap();
        if let ColumnChunk::I64(v) = &batch.columns[0] {
            rows += v.len();
        }
    }
    rows
}

fn bench_zone_map(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    write_parts(dir.path());
    let cols = schema().columns;

    // ts spans 0..1_000_000. Threshold controls how many parts get pruned.
    let configs = [
        ("prune_99pct", 990_000i64), // only the last part survives
        ("prune_50pct", 500_000i64), // half the parts survive
        ("prune_none",  -1i64),      // every part survives — pruning can't help
    ];

    let mut group = c.benchmark_group("zone_map");
    group.throughput(Throughput::Elements(TOTAL_ROWS as u64));

    for (label, threshold) in configs {
        let pred = Predicate::Cmp {
            col: "ts".to_string(),
            op: CmpOp::Gt,
            value: Literal::Int(threshold),
        };

        // Baseline: read every part, then row-filter.
        group.bench_function(BenchmarkId::new("full_scan", label), |b| {
            b.iter(|| {
                let scan = FullScan::new(dir.path().to_path_buf(), cols.clone()).unwrap();
                let node: Box<dyn Processor> =
                    Box::new(Filter::new(Box::new(scan), pred.clone()));
                black_box(drain(node));
            });
        });

        // Pruned: skip parts the zone map rules out, then row-filter.
        group.bench_function(BenchmarkId::new("zone_map_scan", label), |b| {
            b.iter(|| {
                let scan = ZoneMapScan::new(
                    dir.path().to_path_buf(),
                    cols.clone(),
                    pred.clone(),
                )
                .unwrap();
                let node: Box<dyn Processor> =
                    Box::new(Filter::new(Box::new(scan), pred.clone()));
                black_box(drain(node));
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_zone_map);
criterion_main!(benches);
