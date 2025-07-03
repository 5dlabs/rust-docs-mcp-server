#!/usr/bin/env python3
import os
import asyncio
from openai import AsyncOpenAI
import psycopg2
from psycopg2.extras import RealDictCursor
import numpy as np

async def main():
    # Initialize OpenAI client
    client = AsyncOpenAI(api_key=os.environ.get('OPENAI_API_KEY'))
    
    # Connect to database
    db_url = os.environ.get('MCPDOCS_DATABASE_URL', 'postgresql://jonathonfritz@localhost/rust_docs_vectors')
    conn = psycopg2.connect(db_url)
    cur = conn.cursor(cursor_factory=RealDictCursor)
    
    # Test query
    query = "How do I create routes in axum and what are the different ways to define route handlers?"
    print(f"Query: {query}\n")
    
    # Generate embedding for the query
    print("Generating embedding for query...")
    response = await client.embeddings.create(
        model="text-embedding-3-small",
        input=query
    )
    query_embedding = response.data[0].embedding
    
    # Convert to PostgreSQL array format
    embedding_str = '[' + ','.join(map(str, query_embedding)) + ']'
    
    # Search in database
    print("\nSearching in database...")
    cur.execute("""
        SELECT doc_path, 
               SUBSTRING(content, 1, 500) as content_preview,
               1 - (embedding <=> %s::vector) as similarity
        FROM doc_embeddings
        WHERE crate_name = 'axum'
        ORDER BY embedding <=> %s::vector
        LIMIT 10
    """, (embedding_str, embedding_str))
    
    results = cur.fetchall()
    
    print("\nTop 10 results:")
    for i, r in enumerate(results):
        print(f"\n--- Result {i+1} (similarity: {r['similarity']:.4f}) ---")
        print(f"Path: {r['doc_path']}")
        print(f"Content: {r['content_preview'][:200]}...")
    
    # Check if any Router docs are in top results
    print("\n\nRouter-specific documents in results:")
    router_found = False
    for r in results:
        if 'Router' in r['doc_path'] or 'router' in r['doc_path'].lower():
            print(f"  - {r['doc_path']} (similarity: {r['similarity']:.4f})")
            router_found = True
    
    if not router_found:
        print("  None found in top 10 results")
        
        # Let's check where Router docs rank
        print("\n\nSearching for Router documentation ranking...")
        cur.execute("""
            WITH ranked AS (
                SELECT doc_path,
                       1 - (embedding <=> %s::vector) as similarity,
                       ROW_NUMBER() OVER (ORDER BY embedding <=> %s::vector) as rank
                FROM doc_embeddings
                WHERE crate_name = 'axum'
            )
            SELECT * FROM ranked
            WHERE doc_path ILIKE '%%router%%'
            ORDER BY rank
            LIMIT 5
        """, (embedding_str, embedding_str))
        
        router_results = cur.fetchall()
        for r in router_results:
            print(f"  Rank #{r['rank']}: {r['doc_path']} (similarity: {r['similarity']:.4f})")
    
    cur.close()
    conn.close()

if __name__ == "__main__":
    asyncio.run(main())