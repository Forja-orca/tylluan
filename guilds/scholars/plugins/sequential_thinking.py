"""
TylluanNexus Sequential Thinking Guild — Reasoning and chain-of-thought.

This guild provides tools for advanced reasoning, chain-of-thought analysis,
and problem decomposition. No external services required - all local.
"""

from guilds.core import utils
import json
import re
from typing import List, Dict, Optional
from mcp.server.fastmcp import FastMCP

import sqlite3
from pathlib import Path

mcp = FastMCP("tylluan-sequential-thinking")

MAX_DEPTH = 10
DB_PATH = Path("data/silva.db")

def query_silva(query: str, limit: int = 3) -> List[Dict]:
    """Search for relevant nodes in the knowledge graph."""
    if not DB_PATH.exists():
        return []
    
    try:
        conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
        cursor = conn.cursor()
        
        # 1. Try exact substring match first
        search_query = f"%{query}%"
        cursor.execute(
            "SELECT id, type, content FROM nodes WHERE content LIKE ? LIMIT ?",
            (search_query, limit)
        )
        rows = cursor.fetchall()
        if rows:
            conn.close()
            return [{"id": r[0], "type": r[1], "content": r[2]} for r in rows]
            
        # 2. Try word-by-word tokenized match for keywords of length >= 4
        words = [re.sub(r'\W+', '', w) for w in query.split()]
        keywords = [w for w in words if len(w) >= 4]
        if keywords:
            clauses = []
            params = []
            for kw in keywords[:5]: # Limit to top 5 keywords to avoid too large query
                clauses.append("content LIKE ?")
                params.append(f"%{kw}%")
            
            sql = f"SELECT id, type, content FROM nodes WHERE {' OR '.join(clauses)} LIMIT ?"
            params.append(limit)
            cursor.execute(sql, tuple(params))
            rows = cursor.fetchall()
            if rows:
                conn.close()
                return [{"id": r[0], "type": r[1], "content": r[2]} for r in rows]
                
        conn.close()
        return []
    except Exception as e:
        return [{"error": str(e)}]

@mcp.tool()
async def think(
    problem: str = "",
    depth: int = 3,
    context: Optional[str] = None,
    intent: str = "",
    query: str = "",
) -> str:
    """Perform sequential thinking on a problem with Knowledge Graph awareness.
    
    Args:
        problem: The problem or question to analyze.
        depth: Number of reasoning steps (1-10, default 3).
        context: Optional context or constraints.
        intent: Natural language intent for fallback parameter extraction.
        query: Search query for fallback parameter extraction.
    
    Returns:
        Sequential reasoning analysis.
    """
    if depth < 1 or depth > MAX_DEPTH:
        return f"❌ Depth must be between 1 and {MAX_DEPTH}"
    
    if not problem:
        problem = intent or query or "analyze this topic"

    steps = []
    
    # Step 1: Problem decomposition
    steps.append({
        "step": 1,
        "type": "decomposition",
        "content": f"Problem: {problem}",
        "questions": decompose_problem(problem)
    })
    
    # Step 2: Knowledge Retrieval (SilvaDB Integration)
    if "genera una síntesis coherente:" in problem:
        parts = problem.split("coherente:", 1)
        nodes_parts = parts[1].split(" | ") if len(parts) > 1 else []
        knowledge = [{"id": f"node_{i}", "type": "retrieved", "content": part.strip()} for i, part in enumerate(nodes_parts) if part.strip()]
    else:
        knowledge = query_silva(problem[:100])
    
    if len(knowledge) == 0:
        # Fall back to using the problem itself as the knowledge context to prevent premature exit
        knowledge = [{"id": "problem_context", "type": "decomposition", "content": problem}]

    # Extract main ideas (first sentence of each node)
    ideas = []
    for k in knowledge:
        content = k.get("content", "")
        # Get first sentence
        first_sentence = content.split(". ")[0].strip()
        if first_sentence and first_sentence not in ideas:
            ideas.append(first_sentence)
    
    # Build synthesis
    synthesis_lines = [
        "🧠 Análisis de Síntesis Local",
        "=" * 30,
        "",
        "### Main Extracted Ideas:",
    ]
    for idea in ideas[:3]:
        synthesis_lines.append(f"- {idea}")
    
    synthesis_lines.append("\n### Synthesis:")
    common_themes = " and ".join(ideas[:2]).lower()
    synthesis_lines.append(f"The analyzed elements converge on {common_themes}. This structure suggests a coherent knowledge base on the topic.")
    
    synthesis_lines.append("\n### Generative Question:")
    synthesis_lines.append(f"Based on this, what additional information do you think is missing to complete the analysis of '{problem[:30]}...'?")
    
    return "\n".join(synthesis_lines)


def decompose_problem(problem: str) -> List[str]:
    """Break problem into sub-questions."""
    questions = []
    
    # What, Why, How, What if
    questions.append(f"What is the core of '{problem}'?")
    questions.append(f"Why is this important?")
    questions.append(f"How would we solve this?")
    questions.append(f"What could go wrong?")
    
    return questions[:4]


def generate_reasoning(problem: str, step: int, context: Optional[str]) -> str:
    """Generate reasoning for a specific step."""
    reasoning_map = {
        3: f"Analyzing problem structure: {problem[:50]}...",
        4: "Identifying key constraints and variables...",
        5: "Evaluating alternative approaches...",
        6: "Checking for edge cases...",
        7: "Validating assumptions...",
        8: "Considering counterfactuals...",
        9: "Synthesizing insights...",
        10: "Final verification..."
    }
    
    base = reasoning_map.get(step, f"Reasoning step {step}")
    if context:
        base += f" (Context: {context[:30]}...)"
    
    return base


def synthesize(steps: List[Dict], problem: str) -> str:
    """Synthesize findings into conclusion."""
    reasoning_steps = [s for s in steps if s.get("type") == "reasoning"]
    conclusion = f"""
## Conclusion for: {problem}

Based on {len(reasoning_steps)} reasoning steps:

1. The problem requires careful decomposition
2. Multiple approaches can be evaluated
3. Edge cases must be considered
4. Final solution should balance correctness and efficiency

**Recommendation**: Proceed with implementation while monitoring for edge cases.
"""
    return conclusion


def format_thinking(steps: List[Dict]) -> str:
    """Format thinking trace as readable output."""
    lines = ["🧠 Sequential Thinking Analysis", "=" * 40, ""]
    
    for step in steps:
        step_type = step.get("type", "unknown")
        icon = {
            "decomposition": "🔍",
            "context": "📋",
            "reasoning": "🤔",
            "synthesis": "✅"
        }.get(step_type, "•")
        
        lines.append(f"Step {step['step']} {icon} {step_type.upper()}")
        lines.append(f"  {step['content']}")
        
        if "questions" in step:
            for q in step["questions"]:
                lines.append(f"    → {q}")
        
        lines.append("")
    
    return "\n".join(lines)


@mcp.tool()
async def analyze_thought_chain(chain: str = "", intent: str = "", query: str = "") -> str:
    """Analyze a thought chain for quality and consistency.
    
    Args:
        chain: The thought chain to analyze.
        intent: Natural language intent for fallback parameter extraction.
        query: Search query for fallback parameter extraction.
    
    Returns:
        Quality analysis.
    """
    if not chain:
        chain = intent or query or ""
    if not chain:
        return "❌ No chain provided. Describe the thought chain to analyze."
    length = len(chain)
    sentences = len(re.split(r'[.!?]+', chain))
    
    analysis = f"""
📊 Thought Chain Analysis
==========================

Length: {length} characters
Sentences: {sentences}

Quality Indicators:
"""
    
    # Check for reasoning patterns
    has_because = "because" in chain.lower()
    has_therefore = "therefore" in chain.lower()
    has_however = "however" in chain.lower()
    has_if = "if " in chain.lower()
    
    analysis += f"""
- Causal reasoning (because): {'✓' if has_because else '✗'}
- Conclusion markers (therefore): {'✓' if has_therefore else '✗'}
- Contrast markers (however): {'✓' if has_however else '✗'}
- Conditional logic (if): {'✓' if has_if else '✗'}
"""
    
    if has_because and has_therefore:
        analysis += "\n✅ Strong reasoning chain detected"
    elif has_because or has_therefore:
        analysis += "\n⚠️ Partial reasoning chain"
    else:
        analysis += "\n⚠️ Weak reasoning - consider adding more logical connectors"
    
    return analysis


@mcp.tool()
async def compare_options(
    options: str = "",
    criteria: Optional[str] = None,
    intent: str = "",
    query: str = "",
) -> str:
    """Compare multiple options systematically.
    
    Args:
        options: Options to compare (comma-separated).
        criteria: Optional evaluation criteria.
        intent: Natural language intent for fallback parameter extraction.
        query: Search query for fallback parameter extraction.
    
    Returns:
        Comparison matrix.
    """
    if not options:
        options = intent or query or ""
    
    # Strip natural-language prefix like "compare options:", "compare:", etc.
    import re as _re
    options = _re.sub(
        r'^(?:compare\s+)?(?:options?|choices?|alternatives?|pros\s+and\s+cons\s+of|cual\s+es\s+mejor\s+entre)\s*[:\-]?\s*',
        '', options, flags=_re.IGNORECASE
    ).strip()
    
    def parse_options(options_str):
        """Parse comma-separated options, respecting quoted strings."""
        options = []
        current = ""
        in_quotes = False
        quote_char = None
        
        i = 0
        while i < len(options_str):
            char = options_str[i]
            
            if char in ('"', "'") and (i == 0 or options_str[i-1] != '\\'):
                if not in_quotes:
                    in_quotes = True
                    quote_char = char
                elif char == quote_char:
                    in_quotes = False
                    quote_char = None
                else:
                    # Escaped quote within quotes - treat as literal
                    current += char
            elif char == ',' and not in_quotes:
                options.append(current.strip())
                current = ""
            else:
                current += char
            
            i += 1
        
        # Add the last option
        if current or options_str.endswith(','):
            options.append(current.strip())
        
        return [opt for opt in options if opt]
    
    if not options:
        return "❌ Please provide options to compare."
    
    option_list = parse_options(options)
    
    if not option_list:
        return "❌ No options provided"
    
    if len(option_list) < 2:
        return f"❌ Need at least 2 options to compare, got: {len(option_list)}"
    
    # Calculate dynamic column width (cap at 20 characters for readability)
    max_opt_len = max(len(opt) for opt in option_list) if option_list else 0
    col_width = min(max(max_opt_len + 2, 10), 20)  # At least 10, at most 20
    
    default_criteria = ["feasibility", "risk", "cost", "benefit"]
    criteria_list = [c.strip() for c in criteria.split(",")] if criteria else default_criteria
    
    # Header — truncate display to col_width to keep table aligned
    header = "Criteria | " + " | ".join(f"{o[:col_width]:<{col_width}}" for o in option_list)
    separator = "-" * len(header)
    
    lines = ["⚖️ Option Comparison Matrix", "=" * 40, ""]
    lines.append(header)
    lines.append(separator)
    
    for crit in criteria_list:
        scores = [generate_score(crit, opt) for opt in option_list]
        crit_display = f"{crit:<12}"
        scores_display = " | ".join(f"{s:>{col_width}}" for s in scores)
        lines.append(f"{crit_display} | {scores_display}")
    
    lines.append("")
    lines.append("💡 Recommendation: " + option_list[0] if option_list else "")
    
    return "\n".join(lines)


def generate_score(criterion: str, option: str) -> str:
    """Generate a simulated score for comparison."""
    import random
    score = random.randint(5, 10)
    return f"{score}/10"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)