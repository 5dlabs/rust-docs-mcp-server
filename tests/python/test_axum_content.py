#!/usr/bin/env python3
import psycopg2
import os
from psycopg2.extras import RealDictCursor

# Get database URL
db_url = os.environ.get('MCPDOCS_DATABASE_URL', 'postgresql://jonathonfritz@localhost/rust_docs_vectors')

# Parse connection string
if '@' in db_url:
    parts = db_url.replace('postgresql://', '').split('@')
    user_pass = parts[0].split(':')
    host_db = parts[1].split('/')
    
    user = user_pass[0]
    password = user_pass[1] if len(user_pass) > 1 else None
    host = host_db[0].split(':')[0]
    port = host_db[0].split(':')[1] if ':' in host_db[0] else '5432'
    database = host_db[1]
else:
    user = 'jonathonfritz'
    host = 'localhost'
    port = '5432'
    database = 'rust_docs_vectors'
    password = None

print(f"Connecting to database: {database}")
conn = psycopg2.connect(host=host, port=port, database=database, user=user, password=password)
cur = conn.cursor(cursor_factory=RealDictCursor)

# 1. Check axum documents with "route" in them
print("\n1. Axum documents containing 'route' or 'Router':")
cur.execute("""
    SELECT doc_path, LENGTH(content) as content_len, 
           SUBSTRING(content, 1, 500) as preview
    FROM doc_embeddings 
    WHERE crate_name = 'axum' 
    AND (content ILIKE '%route%' OR content ILIKE '%router%')
    ORDER BY content_len DESC
    LIMIT 5
""")
results = cur.fetchall()
for i, r in enumerate(results):
    print(f"\n--- Document {i+1} ---")
    print(f"Path: {r['doc_path']}")
    print(f"Length: {r['content_len']} chars")
    print(f"Preview: {r['preview'][:200]}...")

# 2. Check if embeddings exist and are valid
print("\n\n2. Embedding validity check:")
cur.execute("""
    SELECT COUNT(*) as total,
           COUNT(embedding) as with_embedding,
           COUNT(*) FILTER (WHERE embedding IS NULL) as null_embeddings
    FROM doc_embeddings
    WHERE crate_name = 'axum'
""")
result = cur.fetchone()
print(f"Total docs: {result['total']}")
print(f"With embeddings: {result['with_embedding']}")
print(f"NULL embeddings: {result['null_embeddings']}")

# 3. Test vector search directly
print("\n\n3. Testing vector search for 'Router' (using a sample embedding):")
cur.execute("""
    SELECT doc_path, content,
           1 - (embedding <=> (SELECT embedding FROM doc_embeddings WHERE crate_name = 'axum' AND content ILIKE '%Router%' LIMIT 1)) as similarity
    FROM doc_embeddings
    WHERE crate_name = 'axum'
    AND embedding IS NOT NULL
    ORDER BY embedding <=> (SELECT embedding FROM doc_embeddings WHERE crate_name = 'axum' AND content ILIKE '%Router%' LIMIT 1)
    LIMIT 5
""")
results = cur.fetchall()
for i, r in enumerate(results):
    print(f"\n--- Result {i+1} (similarity: {r['similarity']:.4f}) ---")
    print(f"Path: {r['doc_path']}")
    print(f"Content preview: {r['content'][:300]}...")

# 4. Compare with tokio
print("\n\n4. Comparison - Tokio documents for reference:")
cur.execute("""
    SELECT doc_path, LENGTH(content) as content_len
    FROM doc_embeddings 
    WHERE crate_name = 'tokio'
    ORDER BY content_len DESC
    LIMIT 5
""")
results = cur.fetchall()
for r in results:
    print(f"  - {r['doc_path']} ({r['content_len']} chars)")

cur.close()
conn.close()
print("\n✅ Analysis complete!")