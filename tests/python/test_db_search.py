#!/usr/bin/env python3
import psycopg2
import os
from psycopg2.extras import RealDictCursor

# Get database URL
db_url = os.environ.get('MCPDOCS_DATABASE_URL', 'postgresql://jonathonfritz@localhost/rust_docs_vectors')

# Parse connection string
if '@' in db_url:
    # Format: postgresql://user:pass@host/db
    parts = db_url.replace('postgresql://', '').split('@')
    user_pass = parts[0].split(':')
    host_db = parts[1].split('/')
    
    user = user_pass[0]
    password = user_pass[1] if len(user_pass) > 1 else None
    host = host_db[0].split(':')[0]
    port = host_db[0].split(':')[1] if ':' in host_db[0] else '5432'
    database = host_db[1]
else:
    # Simple format
    user = 'jonathonfritz'
    host = 'localhost'
    port = '5432'
    database = 'rust_docs_vectors'
    password = None

print(f"Connecting to database: {database} on {host}:{port} as {user}")

# Connect
conn = psycopg2.connect(
    host=host,
    port=port,
    database=database,
    user=user,
    password=password
)

cur = conn.cursor(cursor_factory=RealDictCursor)

# Check axum documents
print("\nChecking axum documents in database:")
cur.execute("""
    SELECT COUNT(*) as count, 
           COUNT(DISTINCT doc_path) as unique_paths,
           MIN(LENGTH(content)) as min_content_len,
           MAX(LENGTH(content)) as max_content_len,
           AVG(LENGTH(content))::int as avg_content_len
    FROM doc_embeddings 
    WHERE crate_name = 'axum'
""")
result = cur.fetchone()
print(f"  Total docs: {result['count']}")
print(f"  Unique paths: {result['unique_paths']}")
print(f"  Content length - Min: {result['min_content_len']}, Max: {result['max_content_len']}, Avg: {result['avg_content_len']}")

# Check for route-related documents
print("\nSearching for route-related documents:")
cur.execute("""
    SELECT doc_path, LENGTH(content) as content_len
    FROM doc_embeddings 
    WHERE crate_name = 'axum' 
    AND (doc_path ILIKE '%route%' OR doc_path ILIKE '%router%' OR content ILIKE '%route%')
    LIMIT 10
""")
results = cur.fetchall()
for r in results:
    print(f"  - {r['doc_path']} ({r['content_len']} chars)")

# Check if embeddings are properly stored
print("\nChecking embedding dimensions:")
cur.execute("""
    SELECT doc_path, array_length(embedding, 1) as dim
    FROM doc_embeddings 
    WHERE crate_name = 'axum'
    LIMIT 5
""")
results = cur.fetchall()
for r in results:
    print(f"  - {r['doc_path']}: {r['dim']} dimensions")

cur.close()
conn.close()
print("\nDatabase check complete!")