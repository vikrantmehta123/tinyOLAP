# Deferred Tasks

These tasks are not definitively in Phase 1 or Phase 2. But
these tasks naturally came up during implementations of the 
currently scoped tasks and could be worth thinking over as 
future additions to the spec.

1. Zero-copy buffering using bytemuck

2. Adaptive Granularity: Currently each granule is fixed at
512-row granules for column types. For long strings, this can
get too big. So ClickHouse implements Adaptive Granularity,
which could be worth expploring. 

3. Delta-Decode and Dictionary Lookups are SIMD paths. So potentially this could be implemented.

4. Memory-bounded SELECT output — promoted to TASK-015 (streaming deferred to after
   the physical plan is in place).