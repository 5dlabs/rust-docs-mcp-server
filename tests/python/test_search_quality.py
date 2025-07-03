#!/usr/bin/env python3
"""Test script to check if § symbols affect search quality"""

import asyncio
import os
import sys
from pathlib import Path
import numpy as np
from dotenv import load_dotenv

# Add the src directory to Python path
sys.path.insert(0, str(Path(__file__).parent / "src"))

# Load environment variables
load_dotenv()

async def test_search_quality():
    # Import necessary modules
    from embeddings import EMBEDDING_CLIENT, init_embedding_provider
    from database import Database
    
    # Initialize embedding provider
    await init_embedding_provider()
    
    # Initialize database
    db = Database()
    await db.new()
    
    # Test queries - some that might be affected by § symbol
    test_queries = [
        ("axum high-level features", "Should find the main index page"),
        ("axum route requests handlers", "Should find routing info"),
        ("axum error handling model", "Should find error handling docs"),
        ("§ High-level features", "Query with § symbol"),
        ("High-level features", "Same query without § symbol"),
    ]
    
    print("Testing search quality with and without § symbols...\n")
    
    embedding_provider = EMBEDDING_CLIENT.get()
    
    for query, description in test_queries:
        print(f"\nQuery: '{query}'")
        print(f"Description: {description}")
        print("-" * 60)
        
        # Generate embedding for query
        embeddings, _ = await embedding_provider.generate_embeddings([query])
        query_embedding = np.array(embeddings[0])
        
        # Search in database
        results = await db.search_similar_docs("axum", query_embedding, 3)
        
        for i, (path, content, score) in enumerate(results):
            # Check if content starts with § or has § near the beginning
            has_symbol = '§' in content[:100]
            symbol_pos = content.find('§')
            
            print(f"\n  Result {i+1}:")
            print(f"    Path: {path}")
            print(f"    Score: {score:.4f}")
            print(f"    Has § symbol: {has_symbol}")
            if symbol_pos >= 0:
                print(f"    § position: {symbol_pos}")
            print(f"    Content preview: {content[:150].replace(chr(10), ' ')}...")

if __name__ == "__main__":
    asyncio.run(test_search_quality())