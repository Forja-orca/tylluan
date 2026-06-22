"""
TylluanNexus Scholar Guild — PDF Tools.

Provides tools for extracting text, metadata, and merging PDF documents
locally without external cloud APIs.

Status: operational (degraded: requires pip install mcp fastmcp pypdf)
Last verified: 2026-05-11
"""

import os
import sys
from typing import Optional

try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ PDF Guild: MCP dependencies not available: {e}", file=sys.stderr)

if _MCP_AVAILABLE:
    try:
        from pypdf import PdfReader, PdfWriter
        _PDF_AVAILABLE = True
    except ImportError:
        _PDF_AVAILABLE = False
else:
    _PDF_AVAILABLE = False

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-pdf")
    
    @mcp.tool()
    async def pdf_extract_text(
        path: str,
        page_start: Optional[int] = None,
        page_end: Optional[int] = None,
    ) -> str:
        """Extract text content from a local PDF file.

        Args:
            path: Absolute path to the PDF file.
            page_start: Optional starting page (0-indexed).
            page_end: Optional ending page (0-indexed, exclusive).
        """
        if not _PDF_AVAILABLE:
            return "❌ PDF tools unavailable. Install: pip install pypdf"
        if not os.path.exists(path):
            return f"❌ Error: File not found: {path}"

        try:
            reader = PdfReader(path)
            text = ""
            
            start = page_start or 0
            end = page_end or len(reader.pages)
            
            for i in range(start, min(end, len(reader.pages))):
                page = reader.pages[i]
                text += f"--- Page {i+1} ---\n"
                text += page.extract_text() + "\n\n"

            return text or "⚠️ No text extracted (could be a scanned document)."
        except Exception as e:
            return f"❌ PDF Error: {e}"

    @mcp.tool()
    async def pdf_info(path: str) -> str:
        """Get metadata and page count for a PDF file.

        Args:
            path: Absolute path to the PDF file.
        """
        if not _PDF_AVAILABLE:
            return "❌ PDF tools unavailable. Install: pip install pypdf"
        if not os.path.exists(path):
            return f"❌ Error: File not found: {path}"

        try:
            reader = PdfReader(path)
            meta = reader.metadata
            info = {
                "pages": len(reader.pages),
                "author": meta.get("/Author", "Unknown"),
                "creator": meta.get("/Creator", "Unknown"),
                "producer": meta.get("/Producer", "Unknown"),
                "subject": meta.get("/Subject", "Unknown"),
                "title": meta.get("/Title", "Unknown"),
            }
            return f"📄 PDF Info: {info}"
        except Exception as e:
            return f"❌ PDF Error: {e}"

    @mcp.tool()
    async def pdf_merge(paths: list[str], output_path: str) -> str:
        """Merge multiple PDF files into one.

        Args:
            paths: List of absolute paths to PDF files to merge.
            output_path: Path where the merged PDF will be saved.
        """
        if not _PDF_AVAILABLE:
            return "❌ PDF tools unavailable. Install: pip install pypdf"
        try:
            writer = PdfWriter()
            for path in paths:
                if not os.path.exists(path):
                    return f"❌ Error: File not found: {path}"
                writer.append(path)

            with open(output_path, "wb") as f:
                writer.write(f)

            return f"✅ Successfully merged {len(paths)} files into {output_path}."
        except Exception as e:
            return f"❌ Merge Error: {e}"

    if __name__ == "__main__":
        utils.safe_mcp_run(mcp)
else:
    # Stub when MCP not available - allow module to load but respond to tools
    def _stub_tool(*args, **kwargs):
        return "❌ PDF guild unavailable. Install dependencies: pip install mcp fastmcp pypdf"
    
    # Provide a minimal callable for registry
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    
    mcp = StubMCP()
    
    async def pdf_extract_text(path: str, page_start: Optional[int] = None, page_end: Optional[int] = None) -> str:
        return _stub_tool()
    
    async def pdf_info(path: str) -> str:
        return _stub_tool()
    
    async def pdf_merge(paths: list[str], output_path: str) -> str:
        return _stub_tool()
    
    # Allow module to be imported without crashing
    if __name__ == "__main__":
        print("PDF Guild: MCP not available. Install: pip install mcp fastmcp pypdf")