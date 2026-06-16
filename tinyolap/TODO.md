This document lists all the things that **aren't** in tinyOLAP today but could be added.

1. Server:
    - No need for a fancy authentication, etc. 
    - We only need a long running process so that background tasks can run and that exposes a CLI as is being done today.

2. Query Context:
    - For each query, maintain some statistics like rows read, memory used, time taken, etc.
    - Maybe store this in a system table.

3. Logging:
    - Add detailed logs

4. Make compression optional
    - Right now, lz4 compression is mandatory. But it is possible that at a certain scale, querying non-compressed data is faster. So ideally, if the user is okay with more storage, we need to let him decide whether he wants to compress or not.

5. Background Merges:
    - Scheduler of merges
    - Part Level Statistics need to be captured & stored somewhere
    - Deletion of non-used, unpinned parts.

6. Indexes:
    - Support for primary indexes in the schema

7. Partitions:
    - Support for partitions in the schema
    - Query and storage of data according to partitions.

8. Query Adaptive Background Merges:
    - Allow a configuration that the user can set then the database can decide what queries to precompute when doing background merges.
    - Idea is: use query log, and then check what queries occur frequently and precompute their results when doing compaction.
    - The assumption is: once queries are settled, they don't change much.

9. Push based query execution
    - Currently, we have pull based. But push based is more SIMD friendly.

10. Support for LIMIT, ORDER BY, HAVING clauses. Perhaps subqueries as well.

11. Use DataFusion as query engine

12. Concurrency Playground:
    - Implement RwLock

13. Threadpool in Query Execution
    - Even though we always execute at most one query at a time, we want to have a threadpool for the query that is being executed. 
    - Currently, the gather operator spawns four threads each time a query comes. We want to change this. We want to have a threadpool at startup and then query execution should submit tasks to this pool. 
    - In the insert operator, we use rayon for parallel inserts. For the inserts as well, there may be some threadpool.

