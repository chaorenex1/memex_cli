# Memory Sync API Specification

## Overview

The Memory Sync API provides bidirectional synchronization between local LanceDB storage and remote HTTP server. It supports QA items, validation records, and hit records synchronization.

**Base URL**: `https://api.example.com/v1/qa`

**Authentication**: Bearer Token (optional, configured via `api_key`)

---

## Table of Contents

- [Health Check](#1-health-check)
- [Upsert QA Items](#2-upsert-qa-items)
- [Download Updates](#3-download-updates)
- [Upload Validations](#4-upload-validations)
- [Upload Hits](#5-upload-hits)
- [Data Models](#data-models)
- [Error Responses](#error-responses)

---

## 1. Health Check

Check if the remote memory service is accessible.

### Request

```
GET /v1/qa/health
Authorization: Bearer {api_key}
```

### Response

**Status**: `200 OK`

```json
{
  "status": "healthy",
  "timestamp": "2024-01-22T10:30:00Z"
}
```

---

## 2. Upsert QA Items

Upload a batch of QA items to the remote server. Existing items are updated, new items are created.

### Request

```
POST /v1/qa/sync/upsert
Authorization: Bearer {api_key}
Content-Type: application/json
```

**Request Body**:

```json
{
  "items": [
    {
      "id": "qa_123abc",
      "project_id": "proj_xyz",
      "question": "How do I implement async in Rust?",
      "answer": "Use the `async` and `await` keywords with tokio runtime...",
      "tags": ["rust", "async", "tokio"],
      "confidence": 0.85,
      "validation_level": 1,
      "source": "manual",
      "author": "user@example.com",
      "metadata": {"lang": "en", "category": "programming"},
      "created_at": "2024-01-22T10:00:00Z",
      "updated_at": "2024-01-22T10:00:00Z"
    }
  ]
}
```

### Response

**Status**: `200 OK`

```json
{
  "id_mapping": [
    ["local_id_1", "remote_id_1"],
    ["local_id_2", "remote_id_2"]
  ],
  "failed": ["local_id_3"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id_mapping` | array | Array of `[local_id, remote_id]` pairs |
| `failed` | array | List of local IDs that failed to upload |

### Error Response

**Status**: `400 Bad Request` / `500 Internal Server Error`

```json
{
  "error": "Invalid request payload",
  "details": "Missing required field: question"
}
```

---

## 3. Download Updates

Download QA items that have been updated since a given timestamp.

### Request

```
GET /v1/qa/sync/updates?since=2024-01-22T09:00:00Z
Authorization: Bearer {api_key}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `since` | string | Yes | RFC3339 timestamp |

### Response

**Status**: `200 OK`

```json
[
  {
    "id": "remote_qa_1",
    "project_id": "proj_xyz",
    "question": "Updated question text",
    "answer": "Updated answer text",
    "tags": ["tag1", "tag2"],
    "confidence": 0.9,
    "validation_level": 2,
    "source": "sync",
    "author": null,
    "metadata": {},
    "created_at": "2024-01-21T10:00:00Z",
    "updated_at": "2024-01-22T10:30:00Z"
  }
]
```

**Empty Response** (no updates):

```json
[]
```

---

## 4. Upload Validations

Upload validation records for QA items.

### Request

```
POST /v1/qa/sync/validations
Authorization: Bearer {api_key}
Content-Type: application/json
```

**Request Body**:

```json
{
  "validations": [
    {
      "id": "val_001",
      "qa_id": "qa_123abc",
      "result": "pass",
      "signal_strength": "strong",
      "success": true,
      "context": {"exit_code": 0, "test_count": 5},
      "created_at": "2024-01-22T10:05:00Z"
    }
  ]
}
```

### Response

**Status**: `200 OK`

```json
{
  "success": true,
  "processed": 1
}
```

---

## 5. Upload Hits

Upload hit records (when QA items are retrieved/shown to users).

### Request

```
POST /v1/qa/sync/hits
Authorization: Bearer {api_key}
Content-Type: application/json
```

**Request Body**:

```json
{
  "hits": [
    {
      "id": "hit_001",
      "qa_id": "qa_123abc",
      "shown": true,
      "used": true,
      "session_id": "sess_abc123",
      "created_at": "2024-01-22T10:06:00Z"
    }
  ]
}
```

### Response

**Status**: `200 OK`

```json
{
  "success": true,
  "processed": 1
}
```

---

## Data Models

### QAItem

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique identifier (UUID) |
| `project_id` | string | Yes | Project namespace |
| `question` | string | Yes | Question text |
| `answer` | string | Yes | Answer text |
| `question_vector` | array<float> | No | Embedding vector (optional for upsert) |
| `tags` | array<string> | No | Category tags |
| `confidence` | float | No | Confidence score (0.0-1.0) |
| `validation_level` | integer | No | 0=Candidate, 1=Verified, 2=Confirmed, 3=Gold |
| `source` | string | No | Source identifier |
| `author` | string | No | Author identifier |
| `metadata` | object | No | Additional metadata |
| `created_at` | datetime | Yes | Creation timestamp (RFC3339) |
| `updated_at` | datetime | Yes | Last update timestamp (RFC3339) |

### ValidationRecord

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique identifier |
| `qa_id` | string | Yes | Reference to QA item |
| `result` | string | No | `"pass"` | `"fail"` | `"unknown"` |
| `signal_strength` | string | No | `"strong"` | `"weak"` |
| `success` | boolean | No | Success flag |
| `context` | object | No | Validation context (JSON) |
| `created_at` | datetime | Yes | Creation timestamp |

### HitRecord

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique identifier |
| `qa_id` | string | Yes | Reference to QA item |
| `shown` | boolean | Yes | Whether item was shown to user |
| `used` | boolean | No | Whether user used the answer |
| `session_id` | string | No | Session identifier |
| `created_at` | datetime | Yes | Creation timestamp |

---

## Error Responses

All endpoints may return the following error responses:

### 400 Bad Request

```json
{
  "error": "Bad Request",
  "message": "Invalid request payload"
}
```

### 401 Unauthorized

```json
{
  "error": "Unauthorized",
  "message": "Invalid or missing API key"
}
```

### 429 Too Many Requests

```json
{
  "error": "Too Many Requests",
  "message": "Rate limit exceeded",
  "retry_after": 60
}
```

### 500 Internal Server Error

```json
{
  "error": "Internal Server Error",
  "message": "An unexpected error occurred"
}
```

---

## Sync Flow Sequence

```
┌─────────┐                    ┌─────────────┐
│  Client │                    │ Remote API  │
└────┬────┘                    └──────┬──────┘
     │                               │
     │  1. GET /health                │
     │──────────────────────────────►│
     │◄──────────────────────────────│ 200 OK
     │                               │
     │  2. POST /sync/upsert          │
     │──────────────────────────────►│
     │◄──────────────────────────────│ 200 + id_mapping
     │                               │
     │  3. GET /sync/updates?since=.. │
     │──────────────────────────────►│
     │◄──────────────────────────────│ 200 + items[]
     │                               │
     │  4. POST /sync/validations     │
     │──────────────────────────────►│
     │◄──────────────────────────────│ 200 OK
     │                               │
     │  5. POST /sync/hits            │
     │──────────────────────────────►│
     │◄──────────────────────────────│ 200 OK
     │                               │
```

---

## Rate Limiting

| Tier | Requests | Window |
|------|----------|--------|
| Free | 100 | 1 hour |
| Pro | 1000 | 1 hour |
| Enterprise | Unlimited | - |

Rate limit headers are included in responses:

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1705922400
```

---

## Implementation Reference

- Client: `plugins/src/memory/sync/remote_client.rs`
- Service: `plugins/src/memory/sync/service.rs`
- Models: `plugins/src/memory/lance/mod.rs`
