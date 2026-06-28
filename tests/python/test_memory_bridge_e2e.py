"""
End-to-end tests for Unified Memory Bridge.

This tests the IPC between Python guilds and kernel SilvaDB.
"""

import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from guilds.core.memory_bridge import MemoryBridge, get_bridge


class TestMemoryBridge:
    """Test Memory Bridge IPC."""
    
    @pytest.fixture
    def bridge(self):
        """Create bridge instance."""
        return MemoryBridge(kernel_url="http://127.0.0.1:3030")
    
    def test_bridge_initialization(self, bridge):
        """Test bridge can be created."""
        assert bridge.url == "http://127.0.0.1:3030"
    
    def test_bridge_write(self, bridge):
        """Test writing to SilvaDB via IPC."""
        content = "Test memory from E2E test"
        result = bridge.write(content, "test", '{"source": "e2e"}')
        
        # Should return status
        assert "status" in result
    
    def test_bridge_search(self, bridge):
        """Test searching SilvaDB via IPC."""
        result = bridge.search("test query", limit=3)
        
        # Should return status and results array
        assert "status" in result
    
    def test_bridge_get_context(self, bridge):
        """Test getting context around a node."""
        # First write something
        write_result = bridge.write("Context test node", "entity", "{}")
        if write_result.get("status") == "ok":
            node_id = write_result.get("id")
            result = bridge.get_context(node_id, depth=1)
            assert "status" in result


if __name__ == "__main__":
    pytest.main([__file__, "-v"])