//! LanceDB local storage implementation.
//!
//! Provides CRUD operations for QA items with vector search capabilities.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;

use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::Table;

use super::embedding::EmbeddingService;
use super::models::{
    HitRecord, QAItem, SignalStrength, SyncStatus, ValidationRecord, ValidationResult,
};
use super::schema::{hit_records_schema, qa_items_schema, validation_records_schema};

/// LanceDB local store for memory data.
pub struct LanceStore {
    db: Connection,
    embedding: Arc<dyn EmbeddingService>,
}

impl LanceStore {
    /// Create or open a LanceDB database at the given path.
    pub async fn new<P: AsRef<Path>>(
        db_path: P,
        embedding: Arc<dyn EmbeddingService>,
    ) -> Result<Self> {
        let db_path = db_path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let db = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        let store = Self { db, embedding };

        // Initialize tables
        store.initialize_tables().await?;

        Ok(store)
    }

    /// Initialize all required tables.
    async fn initialize_tables(&self) -> Result<()> {
        let table_names = self.db.table_names().execute().await?;

        // Create qa_items table if it doesn't exist
        if !table_names.contains(&"qa_items".to_string()) {
            tracing::info!("Creating qa_items table in LanceDB");
            let schema = qa_items_schema(self.embedding.dimension());
            // Create empty table with schema - LanceDB will initialize the table
            let empty_batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
                vec![];
            let schema_ref = Arc::new(schema);
            let reader =
                arrow_array::RecordBatchIterator::new(empty_batches.into_iter(), schema_ref);
            self.db.create_table("qa_items", reader).execute().await?;
        }

        // Create validation_records table if it doesn't exist
        if !table_names.contains(&"validation_records".to_string()) {
            tracing::info!("Creating validation_records table in LanceDB");
            let schema = validation_records_schema();
            let empty_batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
                vec![];
            let schema_ref = Arc::new(schema);
            let reader =
                arrow_array::RecordBatchIterator::new(empty_batches.into_iter(), schema_ref);
            self.db
                .create_table("validation_records", reader)
                .execute()
                .await?;
        }

        // Create hit_records table if it doesn't exist
        if !table_names.contains(&"hit_records".to_string()) {
            tracing::info!("Creating hit_records table in LanceDB");
            let schema = hit_records_schema();
            let empty_batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
                vec![];
            let schema_ref = Arc::new(schema);
            let reader =
                arrow_array::RecordBatchIterator::new(empty_batches.into_iter(), schema_ref);
            self.db
                .create_table("hit_records", reader)
                .execute()
                .await?;
        }

        Ok(())
    }

    /// Get or open the qa_items table.
    async fn qa_table(&self) -> Result<Table> {
        self.db
            .open_table("qa_items")
            .execute()
            .await
            .context("Failed to open qa_items table")
    }

    /// Insert or update a QA item.
    pub async fn upsert_qa(&self, mut item: QAItem) -> Result<QAItem> {
        tracing::info!(
            "Upserting QA item: id={}, project_id={}, is_vectorized={}",
            item.id,
            item.project_id,
            item.is_vectorized
        );

        // Generate embedding if not present
        if item.question_vector.is_none() || !item.is_vectorized {
            tracing::debug!(
                "Generating embedding for item: {}, question_len={}",
                item.id,
                item.question.len()
            );
            let vector = self
                .embedding
                .embed(&item.question)
                .await
                .with_context(|| {
                    format!(
                        "Failed to generate embedding for QA item {}: '{}'",
                        item.id,
                        Self::truncate_string(&item.question, 50)
                    )
                })?;
            item.question_vector = Some(vector);
            item.is_vectorized = true;
        }

        item.mark_modified();

        // Convert to RecordBatch and insert
        let batch = self
            .qa_item_to_batch(&item)
            .with_context(|| format!("Failed to convert QA item {} to RecordBatch", item.id))?;
        let table = self
            .qa_table()
            .await
            .context("Failed to open qa_items table")?;
        let schema = table
            .schema()
            .await
            .context("Failed to get qa_items table schema")?;

        // LanceDB uses versioning - we can just add the new version
        let batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
            vec![Ok(batch)];
        let reader = arrow_array::RecordBatchIterator::new(batches.into_iter(), schema);

        tracing::debug!(
            "Adding QA item to LanceDB table: id={}, question_len={}, answer_len={}",
            item.id,
            item.question.len(),
            item.answer.len()
        );

        match table.add(reader).execute().await {
            Ok(_) => {
                tracing::debug!("QA item upserted successfully: {}", item.id);
                Ok(item)
            }
            Err(e) => {
                // Log the full error chain for debugging
                tracing::error!(
                    "LanceDB add failed for item {}: source_error={}, full_chain={:?}",
                    item.id,
                    e,
                    e
                );
                Err(
                    anyhow::anyhow!("LanceDB add operation failed: {}", e).context(format!(
                        "Failed to upsert QA item {} into LanceDB (id: {}, project_id: {})",
                        item.id, item.id, item.project_id
                    )),
                )
            }
        }
    }

    /// Truncate a string to a maximum length for logging.
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }

    /// Get a QA item by ID.
    pub async fn get_qa(&self, id: &str) -> Result<Option<QAItem>> {
        tracing::debug!("Getting QA item by id: {}", id);
        let table = self.qa_table().await?;

        // Execute full query and filter by id in results
        let results = table.query().execute().await?;

        // Parse results
        let batches = results
            .try_collect::<Vec<arrow_array::RecordBatch>>()
            .await?;
        if batches.is_empty() || batches[0].num_rows() == 0 {
            return Ok(None);
        }

        // Find the item with matching id
        for batch in &batches {
            for row in 0..batch.num_rows() {
                let item = self.batch_to_qa_item(batch, row)?;
                if item.id == id {
                    tracing::debug!("Found QA item: {}", id);
                    return Ok(Some(item));
                }
            }
        }

        tracing::debug!("QA item not found: {}", id);
        Ok(None)
    }

    /// Search for QA items by semantic similarity.
    pub async fn search(
        &self,
        project_id: &str,
        query: &str,
        limit: usize,
        min_score: f32,
    ) -> Result<Vec<(QAItem, f32)>> {
        tracing::info!(
            "Starting vector search: project_id={}, query_len={}, limit={}, min_score={}",
            project_id,
            query.len(),
            limit,
            min_score
        );

        // Generate query vector
        let query_vector = self.embedding.embed(query).await?;
        tracing::debug!(
            "Generated query vector with dimension: {}",
            query_vector.len()
        );

        let table = self.qa_table().await?;

        // Check if table has any data before attempting vector search
        let count_result = table.query().execute().await;
        let has_data = match count_result {
            Ok(results) => {
                let batches = results
                    .try_collect::<Vec<arrow_array::RecordBatch>>()
                    .await?;
                !batches.is_empty() && batches.iter().any(|b| b.num_rows() > 0)
            }
            Err(_) => false,
        };

        if !has_data {
            tracing::info!("QA items table is empty, returning empty search results");
            return Ok(Vec::new());
        }

        // Perform vector search using the query builder
        // LanceDB 0.23+ uses a builder pattern for vector search
        let results = table
            .query()
            .nearest_to(query_vector)?
            .limit(limit)
            .execute()
            .await
            .context("Failed to execute vector search")?;

        tracing::debug!("Vector search completed, processing results");

        // Parse and filter by project_id
        let mut results_with_scores = Vec::new();
        let batches = results
            .try_collect::<Vec<arrow_array::RecordBatch>>()
            .await?;

        for batch in &batches {
            // Extract distance column from LanceDB vector search results
            // LanceDB adds a "_distance" column to vector search results
            let distance_col = batch
                .column_by_name("_distance")
                .and_then(|col| col.as_any().downcast_ref::<arrow_array::Float32Array>());

            for row in 0..batch.num_rows() {
                let item = self.batch_to_qa_item(batch, row)?;
                // Filter by project_id
                if item.project_id.as_str() != project_id {
                    continue;
                }

                // Extract and convert distance to similarity score
                // Distance ranges from 0 (identical) to larger values (dissimilar)
                // Convert to similarity score in range [0, 1] using: 1 / (1 + distance)
                let score = if let Some(distances) = distance_col {
                    let distance = distances.value(row);
                    1.0 / (1.0 + distance) // Convert distance to similarity
                } else {
                    // Fallback if _distance column not found
                    0.8
                };

                if score >= min_score {
                    results_with_scores.push((item, score));
                }
            }
        }

        tracing::info!(
            "Vector search completed: found {} matches (min_score: {})",
            results_with_scores.len(),
            min_score
        );

        Ok(results_with_scores)
    }

    /// Get all items pending sync.
    pub async fn get_pending_sync(&self) -> Result<Vec<QAItem>> {
        tracing::debug!("Getting items pending sync");
        let table = self.qa_table().await?;

        // Use LanceDB's only_if() filter to push down filtering to the database level
        // This avoids loading all items into memory and filtering in Rust
        let results = table
            .query()
            .only_if("sync_status == 'pending'")
            .execute()
            .await?;

        let mut items = Vec::new();
        let batches = results
            .try_collect::<Vec<arrow_array::RecordBatch>>()
            .await?;

        for batch in &batches {
            for row in 0..batch.num_rows() {
                let item = self.batch_to_qa_item(batch, row)?;
                items.push(item);
            }
        }

        tracing::debug!("Found {} items pending sync", items.len());
        Ok(items)
    }

    /// Count items pending sync.
    pub async fn count_pending_sync(&self) -> Result<usize> {
        tracing::debug!("Counting items pending sync");
        let table = self.qa_table().await?;

        // Use LanceDB's only_if() filter to push down counting to the database level
        let results = table
            .query()
            .only_if("sync_status == 'pending'")
            .execute()
            .await?;

        let mut count = 0;
        let batches = results.try_collect::<Vec<_>>().await?;
        for batch in &batches {
            count += batch.num_rows();
        }

        tracing::debug!("Counted {} items pending sync", count);
        Ok(count)
    }

    /// Count all QA items in the store.
    pub async fn count_all(&self) -> Result<usize> {
        tracing::debug!("Counting all QA items in store");
        let table = self.qa_table().await?;

        let results = table.query().execute().await?;
        let batches = results.try_collect::<Vec<_>>().await?;

        let count: usize = batches.iter().map(|b| b.num_rows()).sum();
        tracing::info!("Total QA items count: {}", count);
        Ok(count)
    }

    /// Export all QA items to a JSON writer.
    pub async fn export_qa<W: tokio::io::AsyncWriteExt + Unpin>(
        &self,
        writer: &mut W,
    ) -> Result<()> {
        tracing::info!("Starting QA items export");

        let table = self.qa_table().await?;
        let results = table.query().execute().await?;
        let batches = results.try_collect::<Vec<_>>().await?;

        let mut items = Vec::new();
        for batch in &batches {
            for row in 0..batch.num_rows() {
                if let Ok(item) = self.batch_to_qa_item(batch, row) {
                    items.push(item);
                }
            }
        }

        tracing::info!("Exporting {} QA items to JSONL", items.len());

        // Write as JSONL (one JSON object per line)
        for item in &items {
            let json_str = serde_json::to_string(item)?;
            writer.write_all(json_str.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }

        tracing::info!("Export completed: {} items written", items.len());
        Ok(())
    }

    /// Import QA items from a JSON reader.
    pub async fn import_qa<R: tokio::io::AsyncBufReadExt + Unpin>(
        &self,
        reader: &mut R,
        skip_existing: bool,
    ) -> Result<usize> {
        tracing::info!("Starting QA items import: skip_existing={}", skip_existing);

        let mut imported = 0;
        let mut skipped = 0;
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<QAItem>(trimmed) {
                Ok(mut item) => {
                    // Check if item already exists
                    if skip_existing {
                        if let Ok(Some(_)) = self.get_qa(&item.id).await {
                            skipped += 1;
                            continue; // Skip existing item
                        }
                    }

                    // Reset sync status to pending for imported items
                    item.sync_status = SyncStatus::Pending;
                    item.remote_id = None;
                    item.synced_at = None;

                    self.upsert_qa(item).await?;
                    imported += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to parse QA item: {}, line: {}", e, trimmed);
                }
            }
        }

        tracing::info!(
            "Import completed: imported={}, skipped={}",
            imported,
            skipped
        );

        Ok(imported)
    }

    /// Mark items as synced.
    pub async fn mark_synced(&self, ids: Vec<String>, remote_ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        tracing::info!("Batch marking {} items as synced", ids.len());

        // Fetch all items at once using only_if with ID filter
        let table = self.qa_table().await?;
        let id_filter = ids
            .iter()
            .map(|id| format!("id == '{}'", Self::escape_lancedb_string(id)))
            .collect::<Vec<_>>()
            .join(" OR ");

        let results = table.query().only_if(&id_filter).execute().await?;

        let batches = results.try_collect::<Vec<_>>().await?;

        // Build a map of id -> remote_id for quick lookup
        let remote_map: std::collections::HashMap<String, String> =
            ids.into_iter().zip(remote_ids.into_iter()).collect();

        // Collect items to update
        let mut items_to_update = Vec::new();
        for batch in &batches {
            for row in 0..batch.num_rows() {
                if let Ok(item) = self.batch_to_qa_item(batch, row) {
                    if let Some(remote_id) = remote_map.get(&item.id) {
                        let mut updated = item;
                        updated.mark_synced(Some(remote_id.clone()));
                        items_to_update.push(updated);
                    }
                }
            }
        }

        // Batch upsert all updated items
        if !items_to_update.is_empty() {
            let schema = table.schema().await?;
            let batches: Vec<arrow_array::RecordBatch> = items_to_update
                .iter()
                .map(|item| self.qa_item_to_batch(item))
                .collect::<Result<Vec<_>, _>>()?;

            let reader = arrow_array::RecordBatchIterator::new(batches.into_iter().map(Ok), schema);
            table.add(reader).execute().await?;
        }

        tracing::debug!("Marked {} items as synced", items_to_update.len());
        Ok(())
    }

    /// Add a validation record.
    pub async fn add_validation(&self, validation: ValidationRecord) -> Result<()> {
        tracing::debug!("Adding validation record for qa_id: {}", validation.qa_id);
        let batch = self.validation_to_batch(&validation)?;
        let table = self.db.open_table("validation_records").execute().await?;
        let schema = table.schema().await?;
        let batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
            vec![Ok(batch)];
        let reader = arrow_array::RecordBatchIterator::new(batches.into_iter(), schema);
        table.add(reader).execute().await?;
        tracing::debug!("Validation record added: {}", validation.id);
        Ok(())
    }

    /// Add a hit record.
    pub async fn add_hit(&self, hit: HitRecord) -> Result<()> {
        tracing::debug!("Adding hit record for qa_id: {}", hit.qa_id);
        let batch = self.hit_to_batch(&hit)?;
        let table = self.db.open_table("hit_records").execute().await?;
        let schema = table.schema().await?;
        let batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
            vec![Ok(batch)];
        let reader = arrow_array::RecordBatchIterator::new(batches.into_iter(), schema);
        table.add(reader).execute().await?;
        tracing::debug!("Hit record added: {}", hit.id);
        Ok(())
    }

    /// Get validation records pending sync.
    pub async fn get_pending_validations(&self) -> Result<Vec<ValidationRecord>> {
        tracing::debug!("Getting validation records pending sync");
        let table = self.db.open_table("validation_records").execute().await?;

        // Use LanceDB's only_if() filter for database-level filtering
        let results = table
            .query()
            .only_if("sync_status == 'pending'")
            .execute()
            .await?;

        let mut records = Vec::new();
        let batches = results.try_collect::<Vec<_>>().await?;

        for batch in &batches {
            for row in 0..batch.num_rows() {
                let record = self.batch_to_validation_record(batch, row)?;
                records.push(record);
            }
        }

        tracing::debug!("Found {} validation records pending sync", records.len());
        Ok(records)
    }

    /// Get hit records pending sync.
    pub async fn get_pending_hits(&self) -> Result<Vec<HitRecord>> {
        tracing::debug!("Getting hit records pending sync");
        let table = self.db.open_table("hit_records").execute().await?;

        // Use LanceDB's only_if() filter for database-level filtering
        let results = table
            .query()
            .only_if("sync_status == 'pending'")
            .execute()
            .await?;

        let mut records = Vec::new();
        let batches = results.try_collect::<Vec<_>>().await?;

        for batch in &batches {
            for row in 0..batch.num_rows() {
                let record = self.batch_to_hit_record(batch, row)?;
                records.push(record);
            }
        }

        tracing::debug!("Found {} hit records pending sync", records.len());
        Ok(records)
    }

    /// Mark validation records as synced.
    pub async fn mark_validations_synced(&self, ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            tracing::debug!("No validation records to mark as synced");
            return Ok(());
        }

        tracing::info!("Marking {} validation records as synced", ids.len());

        let table = self.db.open_table("validation_records").execute().await?;

        // LanceDB 0.23+ supports update operations
        // We update the sync_status to "synced" for each record by ID
        for id in ids {
            // The update API in LanceDB uses a builder pattern:
            // table.update().with_filter("id = '...'").set("sync_status", "synced").execute()

            // Since LanceDB's update API can be tricky with string escaping,
            // we use a simpler approach: delete and re-add with updated status
            if let Ok(Some(mut record)) = self.get_validation_by_id(&id).await {
                record.sync_status = SyncStatus::Synced;

                // Re-add the record (this will update it due to same id)
                let batch = self.validation_to_batch(&record)?;
                let schema = table.schema().await?;
                let batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
                    vec![Ok(batch)];
                let reader = arrow_array::RecordBatchIterator::new(batches.into_iter(), schema);

                // Delete old record first
                let _ = table
                    .delete(&format!("id == '{}'", Self::escape_lancedb_string(&id)))
                    .await;

                // Add updated record
                let _ = table.add(reader).execute().await;
            }
        }

        tracing::debug!("Validation records marked as synced");
        Ok(())
    }

    /// Mark hit records as synced.
    pub async fn mark_hits_synced(&self, ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            tracing::debug!("No hit records to mark as synced");
            return Ok(());
        }

        tracing::info!("Marking {} hit records as synced", ids.len());

        let table = self.db.open_table("hit_records").execute().await?;

        // Similar approach: delete and re-add with updated status
        for id in ids {
            if let Ok(Some(mut record)) = self.get_hit_by_id(&id).await {
                record.sync_status = SyncStatus::Synced;

                let batch = self.hit_to_batch(&record)?;
                let schema = table.schema().await?;
                let batches: Vec<Result<arrow_array::RecordBatch, arrow_schema::ArrowError>> =
                    vec![Ok(batch)];
                let reader = arrow_array::RecordBatchIterator::new(batches.into_iter(), schema);

                // Delete old record first
                let _ = table
                    .delete(&format!("id == '{}'", Self::escape_lancedb_string(&id)))
                    .await;

                // Add updated record
                let _ = table.add(reader).execute().await;
            }
        }

        tracing::debug!("Hit records marked as synced");
        Ok(())
    }

    /// Get a validation record by ID.
    async fn get_validation_by_id(&self, id: &str) -> Result<Option<ValidationRecord>> {
        tracing::debug!("Getting validation record by id: {}", id);
        let table = self.db.open_table("validation_records").execute().await?;

        // Query with filter for specific ID
        let results = table
            .query()
            .only_if(format!("id == '{}'", Self::escape_lancedb_string(id)))
            .execute()
            .await?;

        let batches = results.try_collect::<Vec<_>>().await?;

        for batch in &batches {
            for row in 0..batch.num_rows() {
                let record = self.batch_to_validation_record(batch, row)?;
                if record.id == id {
                    return Ok(Some(record));
                }
            }
        }

        Ok(None)
    }

    /// Get a hit record by ID.
    async fn get_hit_by_id(&self, id: &str) -> Result<Option<HitRecord>> {
        tracing::debug!("Getting hit record by id: {}", id);
        let table = self.db.open_table("hit_records").execute().await?;

        // Query with filter for specific ID
        let results = table
            .query()
            .only_if(format!("id == '{}'", Self::escape_lancedb_string(id)))
            .execute()
            .await?;

        let batches = results.try_collect::<Vec<_>>().await?;

        for batch in &batches {
            for row in 0..batch.num_rows() {
                let record = self.batch_to_hit_record(batch, row)?;
                if record.id == id {
                    return Ok(Some(record));
                }
            }
        }

        Ok(None)
    }

    /// Escape strings for LanceDB SQL filter expressions.
    fn escape_lancedb_string(s: &str) -> String {
        // Escape single quotes by doubling them
        s.replace('\'', "''")
    }

    /// Convert QA item to RecordBatch.
    fn qa_item_to_batch(&self, item: &QAItem) -> Result<arrow_array::RecordBatch> {
        use arrow_array::{
            Array, ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, ListArray,
            StringArray, TimestampMillisecondArray, UInt8Array,
        };
        use arrow_buffer::OffsetBuffer;

        let id = StringArray::from(vec![item.id.as_str()]);
        let project_id = StringArray::from(vec![item.project_id.as_str()]);
        let question = StringArray::from(vec![item.question.as_str()]);
        let answer = StringArray::from(vec![item.answer.as_str()]);

        // Vector column
        let list_size: i32 = self
            .embedding
            .dimension()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Embedding dimension exceeds i32"))?;
        let vector_field = Arc::new(arrow_schema::Field::new(
            "item",
            arrow_schema::DataType::Float32,
            true,
        ));
        let question_vector: ArrayRef = if let Some(ref vec) = item.question_vector {
            let values = Float32Array::from(vec.clone());
            let list = FixedSizeListArray::new(
                Arc::clone(&vector_field),
                list_size,
                Arc::new(values),
                None,
            );
            Arc::new(list)
        } else {
            let empty = vec![0.0f32; list_size as usize];
            let values = Float32Array::from(empty);
            let list = FixedSizeListArray::new(vector_field, list_size, Arc::new(values), None);
            Arc::new(list)
        };

        let tags: Vec<Option<&str>> = item.tags.iter().map(|s| Some(s.as_str())).collect();
        let tags_array = StringArray::from(tags);
        let tags = ListArray::new(
            Arc::new(arrow_schema::Field::new(
                "item",
                arrow_schema::DataType::Utf8,
                true,
            )),
            OffsetBuffer::<i32>::from_lengths([tags_array.len()]),
            Arc::new(tags_array),
            None,
        );

        let confidence = Float32Array::from(vec![item.confidence]);
        let validation_level = UInt8Array::from(vec![u8::from(item.validation_level)]);
        let source = StringArray::from(vec![item.source.as_deref().unwrap_or("")]);
        let author = StringArray::from(vec![item.author.as_deref().unwrap_or("")]);
        let metadata = StringArray::from(vec![
            serde_json::to_string(&item.metadata).unwrap_or_default()
        ]);

        let created_millis = item.created_at.timestamp_millis();
        let updated_millis = item.updated_at.timestamp_millis();
        let created_at = TimestampMillisecondArray::from(vec![created_millis]);
        let updated_at = TimestampMillisecondArray::from(vec![updated_millis]);

        let synced_at = if let Some(ts) = item.synced_at {
            TimestampMillisecondArray::from(vec![ts.timestamp_millis()])
        } else {
            TimestampMillisecondArray::from(vec![Option::<i64>::None])
        };

        let sync_status = StringArray::from(vec![item.sync_status.to_string()]);
        let remote_id = StringArray::from(vec![item.remote_id.as_deref().unwrap_or("")]);
        let is_vectorized = BooleanArray::from(vec![item.is_vectorized]);

        let schema = qa_items_schema(self.embedding.dimension());

        let batch = arrow_array::RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(id) as Arc<dyn arrow_array::Array>,
                Arc::new(project_id),
                Arc::new(question),
                Arc::new(answer),
                question_vector,
                Arc::new(tags),
                Arc::new(confidence),
                Arc::new(validation_level),
                Arc::new(source),
                Arc::new(author),
                Arc::new(metadata),
                Arc::new(created_at),
                Arc::new(updated_at),
                Arc::new(synced_at),
                Arc::new(sync_status),
                Arc::new(remote_id),
                Arc::new(is_vectorized),
            ],
        )?;

        Ok(batch)
    }

    /// Convert RecordBatch row to QA item.
    fn batch_to_qa_item(&self, batch: &arrow_array::RecordBatch, row: usize) -> Result<QAItem> {
        use arrow_array::ListArray;
        use arrow_array::{
            Array, BooleanArray, Float32Array, StringArray, TimestampMillisecondArray, UInt8Array,
        };

        let get_string = |col: usize| -> Option<String> {
            let arr = batch.column(col).as_any().downcast_ref::<StringArray>()?;
            arr.is_valid(row).then(|| arr.value(row).to_string())
        };

        let get_i64 = |col: usize| -> Option<i64> {
            let arr = batch
                .column(col)
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()?;
            arr.is_valid(row).then(|| arr.value(row))
        };

        let id = get_string(0).ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let project_id = get_string(1).ok_or_else(|| anyhow::anyhow!("Missing project_id"))?;
        let question = get_string(2).ok_or_else(|| anyhow::anyhow!("Missing question"))?;
        let answer = get_string(3).ok_or_else(|| anyhow::anyhow!("Missing answer"))?;

        // Extract vector from FixedSizeListArray (column 4)
        let question_vector = if batch.num_columns() > 4 {
            batch
                .column(4)
                .as_any()
                .downcast_ref::<arrow_array::FixedSizeListArray>()
                .and_then(|arr| {
                    if arr.is_valid(row) {
                        arr.value(row)
                            .as_any()
                            .downcast_ref::<arrow_array::Float32Array>()
                            .map(|float_arr| float_arr.values().to_vec())
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        // Extract tags from ListArray (column 5)
        let tags = if batch.num_columns() > 5 {
            batch
                .column(5)
                .as_any()
                .downcast_ref::<ListArray>()
                .and_then(|arr| {
                    if arr.is_valid(row) {
                        arr.value(row)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .map(|str_arr| {
                                (0..str_arr.len())
                                    .filter(|&i| str_arr.is_valid(i))
                                    .map(|i| str_arr.value(i).to_string())
                                    .collect()
                            })
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Extract confidence from Float32Array (column 6)
        let confidence = if batch.num_columns() > 6 {
            batch
                .column(6)
                .as_any()
                .downcast_ref::<Float32Array>()
                .and_then(|arr| arr.is_valid(row).then(|| arr.value(row)))
                .unwrap_or(0.5)
        } else {
            0.5
        };

        // Extract validation_level from UInt8Array (column 7)
        let validation_level = if batch.num_columns() > 7 {
            batch
                .column(7)
                .as_any()
                .downcast_ref::<UInt8Array>()
                .and_then(|arr| arr.is_valid(row).then(|| arr.value(row)))
                .unwrap_or(0)
        } else {
            0
        };

        let source = if batch.num_columns() > 8 {
            get_string(8)
        } else {
            None
        };
        let author = if batch.num_columns() > 9 {
            get_string(9)
        } else {
            None
        };

        // Extract metadata from JSON string (column 10)
        let metadata = if batch.num_columns() > 10 {
            get_string(10)
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let created_at = if batch.num_columns() > 11 {
            chrono::DateTime::from_timestamp_millis(get_i64(11).unwrap_or(0))
                .unwrap_or_else(chrono::Utc::now)
        } else {
            chrono::Utc::now()
        };

        let updated_at = if batch.num_columns() > 12 {
            chrono::DateTime::from_timestamp_millis(get_i64(12).unwrap_or(0))
                .unwrap_or_else(chrono::Utc::now)
        } else {
            chrono::Utc::now()
        };

        let synced_at = if batch.num_columns() > 13 {
            get_i64(13).and_then(chrono::DateTime::from_timestamp_millis)
        } else {
            None
        };

        let sync_status = if batch.num_columns() > 14 {
            get_string(14)
                .and_then(|s| s.parse().ok())
                .unwrap_or(SyncStatus::LocalOnly)
        } else {
            SyncStatus::LocalOnly
        };

        let remote_id = if batch.num_columns() > 15 {
            get_string(15)
        } else {
            None
        };

        // Extract is_vectorized from BooleanArray (column 16)
        let is_vectorized = if batch.num_columns() > 16 {
            batch
                .column(16)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .and_then(|arr| arr.is_valid(row).then(|| arr.value(row)))
                .unwrap_or(true)
        } else {
            true
        };

        Ok(QAItem {
            id,
            project_id,
            question,
            answer,
            question_vector,
            tags,
            confidence,
            validation_level: validation_level.into(),
            source,
            author,
            metadata,
            created_at,
            updated_at,
            synced_at,
            sync_status,
            remote_id,
            is_vectorized,
        })
    }

    /// Convert validation record to RecordBatch.
    fn validation_to_batch(
        &self,
        validation: &ValidationRecord,
    ) -> Result<arrow_array::RecordBatch> {
        use arrow_array::{BooleanArray, StringArray, TimestampMillisecondArray};

        let id = StringArray::from(vec![validation.id.as_str()]);
        let qa_id = StringArray::from(vec![validation.qa_id.as_str()]);
        let result = StringArray::from(vec![validation.result.to_string()]);
        let signal_strength = StringArray::from(vec![validation.signal_strength.to_string()]);
        let success = BooleanArray::from(vec![validation.success]);
        let context = StringArray::from(vec![
            serde_json::to_string(&validation.context).unwrap_or_default()
        ]);
        let created_at =
            TimestampMillisecondArray::from(vec![validation.created_at.timestamp_millis()]);
        let sync_status = StringArray::from(vec![validation.sync_status.to_string()]);

        let schema = validation_records_schema();

        let batch = arrow_array::RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(id) as Arc<dyn arrow_array::Array>,
                Arc::new(qa_id),
                Arc::new(result),
                Arc::new(signal_strength),
                Arc::new(success),
                Arc::new(context),
                Arc::new(created_at),
                Arc::new(sync_status),
            ],
        )?;

        Ok(batch)
    }

    /// Convert hit record to RecordBatch.
    fn hit_to_batch(&self, hit: &HitRecord) -> Result<arrow_array::RecordBatch> {
        use arrow_array::{BooleanArray, StringArray, TimestampMillisecondArray};

        let id = StringArray::from(vec![hit.id.as_str()]);
        let qa_id = StringArray::from(vec![hit.qa_id.as_str()]);
        let shown = BooleanArray::from(vec![hit.shown]);
        let used = BooleanArray::from(vec![hit.used]);
        let session_id = StringArray::from(vec![hit.session_id.as_deref().unwrap_or("")]);
        let created_at = TimestampMillisecondArray::from(vec![hit.created_at.timestamp_millis()]);
        let sync_status = StringArray::from(vec![hit.sync_status.to_string()]);

        let schema = hit_records_schema();

        let batch = arrow_array::RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(id) as Arc<dyn arrow_array::Array>,
                Arc::new(qa_id),
                Arc::new(shown),
                Arc::new(used),
                Arc::new(session_id),
                Arc::new(created_at),
                Arc::new(sync_status),
            ],
        )?;

        Ok(batch)
    }

    /// Convert RecordBatch row to ValidationRecord.
    fn batch_to_validation_record(
        &self,
        batch: &arrow_array::RecordBatch,
        row: usize,
    ) -> Result<ValidationRecord> {
        use arrow_array::{Array, BooleanArray, StringArray, TimestampMillisecondArray};

        let get_string = |col: usize| -> Option<String> {
            let arr = batch.column(col).as_any().downcast_ref::<StringArray>()?;
            arr.is_valid(row).then(|| arr.value(row).to_string())
        };

        let get_bool = |col: usize| -> Option<bool> {
            let arr = batch.column(col).as_any().downcast_ref::<BooleanArray>()?;
            arr.is_valid(row).then(|| arr.value(row))
        };

        let get_i64 = |col: usize| -> Option<i64> {
            let arr = batch
                .column(col)
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()?;
            arr.is_valid(row).then(|| arr.value(row))
        };

        let id = get_string(0).ok_or_else(|| anyhow::anyhow!("Missing validation id"))?;
        let qa_id = get_string(1).ok_or_else(|| anyhow::anyhow!("Missing qa_id"))?;
        let result_str = get_string(2).unwrap_or("unknown".to_string());
        let signal_str = get_string(3).unwrap_or("weak".to_string());
        let success = get_bool(4);
        let context_str = get_string(5).unwrap_or("{}".to_string());
        let created_millis = get_i64(6).unwrap_or(0);
        let sync_str = get_string(7).unwrap_or("pending".to_string());

        Ok(ValidationRecord {
            id,
            qa_id,
            result: match result_str.as_str() {
                "pass" => ValidationResult::Pass,
                "fail" => ValidationResult::Fail,
                _ => ValidationResult::Unknown,
            },
            signal_strength: match signal_str.as_str() {
                "strong" => SignalStrength::Strong,
                _ => SignalStrength::Weak,
            },
            success,
            context: serde_json::from_str(&context_str).unwrap_or(serde_json::json!({})),
            created_at: chrono::DateTime::from_timestamp_millis(created_millis)
                .unwrap_or_else(chrono::Utc::now),
            sync_status: match sync_str.as_str() {
                "synced" => SyncStatus::Synced,
                "pending" => SyncStatus::Pending,
                "conflict" => SyncStatus::Conflict,
                "local_only" => SyncStatus::LocalOnly,
                "remote_only" => SyncStatus::RemoteOnly,
                _ => SyncStatus::Pending,
            },
        })
    }

    /// Convert RecordBatch row to HitRecord.
    fn batch_to_hit_record(
        &self,
        batch: &arrow_array::RecordBatch,
        row: usize,
    ) -> Result<HitRecord> {
        use arrow_array::{Array, BooleanArray, StringArray, TimestampMillisecondArray};

        let get_string = |col: usize| -> Option<String> {
            let arr = batch.column(col).as_any().downcast_ref::<StringArray>()?;
            arr.is_valid(row).then(|| arr.value(row).to_string())
        };

        let get_bool = |col: usize| -> Option<bool> {
            let arr = batch.column(col).as_any().downcast_ref::<BooleanArray>()?;
            arr.is_valid(row).then(|| arr.value(row))
        };

        let get_i64 = |col: usize| -> Option<i64> {
            let arr = batch
                .column(col)
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()?;
            arr.is_valid(row).then(|| arr.value(row))
        };

        let id = get_string(0).ok_or_else(|| anyhow::anyhow!("Missing hit id"))?;
        let qa_id = get_string(1).ok_or_else(|| anyhow::anyhow!("Missing qa_id"))?;
        let shown = get_bool(2).unwrap_or(true);
        let used = get_bool(3);
        let session_id = get_string(4).filter(|s| !s.is_empty());
        let created_millis = get_i64(5).unwrap_or(0);
        let sync_str = get_string(6).unwrap_or("pending".to_string());

        Ok(HitRecord {
            id,
            qa_id,
            shown,
            used,
            session_id,
            created_at: chrono::DateTime::from_timestamp_millis(created_millis)
                .unwrap_or_else(chrono::Utc::now),
            sync_status: match sync_str.as_str() {
                "synced" => SyncStatus::Synced,
                "pending" => SyncStatus::Pending,
                "conflict" => SyncStatus::Conflict,
                "local_only" => SyncStatus::LocalOnly,
                "remote_only" => SyncStatus::RemoteOnly,
                _ => SyncStatus::Pending,
            },
        })
    }
}
