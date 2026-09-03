-- The FTS `body` column is a cache of the file-backed Markdown body; `memory_entries` holds
-- only `content_path`, so a SQL trigger cannot read the body. The maintained invariant is that
-- every writer (memory-manager `add`/`update`) explicitly syncs `memory_fts.body`. This
-- migration recreates the AFTER UPDATE trigger as the canonical, body-preserving definition:
-- it refreshes the indexed `title` from the row without ever clobbering the cached `body`.
DROP TRIGGER IF EXISTS memory_fts_au;
CREATE TRIGGER memory_fts_au AFTER UPDATE ON memory_entries BEGIN
  UPDATE memory_fts SET title = new.title WHERE rowid = new.rowid;
END;
