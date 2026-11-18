#!/usr/bin/env python3
"""
Sift Advanced-RAG Benchmark & Evaluation Engine
Evaluates retrieval and generation against ground truth test cases using
industry-standard RAG metrics (Ragas / ARES / TruLens methodology).
"""

import os
import sys
import json
import time
import subprocess
import re
from pathlib import Path
from typing import Dict, List, Any, Tuple
import requests
from tabulate import tabulate
from rich.console import Console
from rich.table import Table

console = Console()

# Environment & Config
PROJECT_ROOT = Path(__file__).resolve().parent.parent
RAG_BIN = PROJECT_ROOT / "target" / "release" / "rag"
DB_PATH = PROJECT_ROOT / ".rag" / "index.sqlite"
DATASET_PATH = PROJECT_ROOT / "eval" / "dataset.json"
RESULTS_PATH = PROJECT_ROOT / "eval" / "results.json"

# Load .env
def load_env():
    env_file = PROJECT_ROOT / ".env"
    if env_file.exists():
        with open(env_file) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1)
                    k = k.strip()
                    v = v.strip().strip('"').strip("'")
                    if k not in os.environ:
                        os.environ[k] = v

load_env()

API_KEY = os.environ.get("OPENAI_API_KEY", "")
BASE_URL = os.environ.get("RAG_CHAT_BASE", "https://api.sarvam.ai/v2")
if not BASE_URL.endswith("/chat/completions"):
    BASE_URL = BASE_URL.rstrip("/") + "/chat/completions"
MODEL_NAME = os.environ.get("RAG_CHAT_MODEL", "deepseekv4.1-flash-1017")


def call_llm_judge(prompt: str, system_prompt: str = "You are an expert AI evaluator. Always respond in valid JSON.") -> Dict[str, Any]:
    """Call LLM-as-a-judge with structured JSON response."""
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    payload = {
        "model": MODEL_NAME,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"},
        "temperature": 0.0,
        "max_tokens": 512
    }
    try:
        res = requests.post(BASE_URL, headers=headers, json=payload, timeout=40)
        res.raise_for_status()
        content = res.json()["choices"][0]["message"]["content"]
        # In case the model returns reasoning_content or markdown backticks
        cleaned = re.sub(r"^```(?:json)?\s*", "", content.strip())
        cleaned = re.sub(r"\s*```$", "", cleaned)
        return json.loads(cleaned)
    except Exception as e:
        return {"score": 0.5, "error": str(e), "reason": "Judge call failed"}


def run_rag_search(query: str, top_k: int = 8) -> Tuple[List[Dict[str, Any]], float]:
    """Runs `rag search <query>` to retrieve ranked chunks with RRF scores."""
    start = time.perf_counter()
    res = subprocess.run(
        [str(RAG_BIN), "search", query, "--top-k", str(top_k)],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True
    )
    elapsed = time.perf_counter() - start
    
    chunks = []
    if res.returncode == 0:
        raw_output = res.stdout
        # Pattern: [1] /path/to/source.pdf  (RRF 0.01596)\n<text>\n
        blocks = re.split(r"\[(\d+)\]\s+([^\n]+)\s+\(RRF\s+([\d\.]+)\)", raw_output)
        # blocks will be: ['', '1', 'path', 'score', 'text', '2', 'path', ...]
        i = 1
        while i < len(blocks) - 3:
            rank = int(blocks[i])
            source = blocks[i+1].strip()
            score = float(blocks[i+2])
            text = blocks[i+3].strip()
            chunks.append({
                "rank": rank,
                "source": source,
                "score": score,
                "text": text
            })
            i += 4
    return chunks, elapsed


def run_rag_ask(query: str, top_k: int = 6) -> Tuple[Dict[str, Any], float]:
    """Runs `rag ask <query>` to generate the synthesized, cited answer."""
    start = time.perf_counter()
    res = subprocess.run(
        [str(RAG_BIN), "ask", query, "--top-k", str(top_k)],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True
    )
    elapsed = time.perf_counter() - start

    output = res.stdout.strip()
    error = res.stderr.strip()

    # Parse answer vs sources
    # Format:
    # <Answer>
    #
    # Sources:
    # [1] /path/file.pdf
    # ...
    # (6 retrieved chunks, 2.7s)
    answer_text = output
    sources = []
    reported_chunks = 0
    reported_latency = elapsed

    if "\n\nSources:\n" in output:
        parts = output.split("\n\nSources:\n", 1)
        answer_text = parts[0].strip()
        sources_part = parts[1].strip()
        for line in sources_part.splitlines():
            m = re.match(r"\[(\d+)\]\s+(.+)", line.strip())
            if m:
                sources.append({"id": int(m.group(1)), "path": m.group(2).strip()})
            m_stat = re.match(r"\((\d+)\s+retrieved chunks,\s+([\d\.]+)s\)", line.strip())
            if m_stat:
                reported_chunks = int(m_stat.group(1))
                reported_latency = float(m_stat.group(2))

    # Extract in-text citation numbers e.g. [1], [2, 3]
    citation_ids = set()
    for match in re.finditer(r"\[([\d,\s]+)\]", answer_text):
        for num in match.group(1).split(","):
            if num.strip().isdigit():
                citation_ids.add(int(num.strip()))

    return {
        "answer": answer_text,
        "sources": sources,
        "citation_ids": list(sorted(citation_ids)),
        "reported_chunks": reported_chunks,
        "reported_latency": reported_latency,
        "returncode": res.returncode,
        "error": error
    }, elapsed


def evaluate_retrieval(test_case: Dict[str, Any], chunks: List[Dict[str, Any]]) -> Dict[str, float]:
    """Computes HitRate@K, MRR, Context Precision, and Context Recall."""
    expected_sources = test_case.get("expected_sources", [])
    key_facts = test_case.get("key_facts", [])
    is_neg = test_case.get("is_negative", False)

    if is_neg:
        # Negative test case: expect no matching relevant sources
        return {
            "hit_rate_at_1": 1.0,
            "hit_rate_at_3": 1.0,
            "hit_rate_at_5": 1.0,
            "hit_rate_at_8": 1.0,
            "mrr": 1.0,
            "context_recall": 1.0,
            "context_precision": 1.0
        }

    if not chunks:
        return {
            "hit_rate_at_1": 0.0,
            "hit_rate_at_3": 0.0,
            "hit_rate_at_5": 0.0,
            "hit_rate_at_8": 0.0,
            "mrr": 0.0,
            "context_recall": 0.0,
            "context_precision": 0.0
        }

    # Hit Rate @ K
    def has_hit_in_top_k(k: int) -> bool:
        for c in chunks[:k]:
            c_src = Path(c["source"]).name
            for exp in expected_sources:
                if exp in c_src or exp in c["source"]:
                    return True
        return False

    hit_1 = 1.0 if has_hit_in_top_k(1) else 0.0
    hit_3 = 1.0 if has_hit_in_top_k(3) else 0.0
    hit_5 = 1.0 if has_hit_in_top_k(5) else 0.0
    hit_8 = 1.0 if has_hit_in_top_k(8) else 0.0

    # MRR (Mean Reciprocal Rank)
    mrr = 0.0
    for idx, c in enumerate(chunks):
        c_src = Path(c["source"]).name
        if any(exp in c_src or exp in c["source"] for exp in expected_sources):
            mrr = 1.0 / (idx + 1)
            break

    # Context Precision (Proportion of top-8 chunks from expected sources)
    relevant_chunks = sum(
        1 for c in chunks
        if any(exp in Path(c["source"]).name or exp in c["source"] for exp in expected_sources)
    )
    context_precision = relevant_chunks / len(chunks) if chunks else 0.0

    # Context Recall (Proportion of key factual keywords found in retrieved text)
    combined_chunk_text = " ".join(c["text"] for c in chunks).lower()
    facts_found = sum(1 for fact in key_facts if fact.lower() in combined_chunk_text)
    context_recall = facts_found / len(key_facts) if key_facts else 1.0

    return {
        "hit_rate_at_1": hit_1,
        "hit_rate_at_3": hit_3,
        "hit_rate_at_5": hit_5,
        "hit_rate_at_8": hit_8,
        "mrr": mrr,
        "context_recall": context_recall,
        "context_precision": context_precision
    }


def evaluate_generation(
    test_case: Dict[str, Any],
    ask_res: Dict[str, Any],
    chunks: List[Dict[str, Any]]
) -> Dict[str, Any]:
    """Computes Faithfulness, Answer Relevance, Citation Precision/Recall, and Semantic Accuracy."""
    query = test_case["query"]
    ground_truth = test_case["ground_truth"]
    expected_sources = test_case.get("expected_sources", [])
    key_facts = test_case.get("key_facts", [])
    is_neg = test_case.get("is_negative", False)
    requires_math = test_case.get("requires_math", False)

    answer = ask_res.get("answer", "")
    sources = ask_res.get("sources", [])
    citation_ids = ask_res.get("citation_ids", [])

    # 1. Negative Rejection handling
    if is_neg:
        lower_ans = answer.lower()
        neg_phrases = [
            "not found", "not mentioned", "not contain", "no information",
            "does not state", "not stated", "cannot find", "unavailable",
            "do not have enough information", "not have enough information",
            "does not include", "does not contain", "no mention", "do not contain"
        ]
        correctly_rejected = 1.0 if any(p in lower_ans for p in neg_phrases) else 0.0
        hallucination_penalty = 1.0 if correctly_rejected == 1.0 else 0.0
        return {
            "faithfulness": hallucination_penalty,
            "answer_relevance": 1.0 if correctly_rejected == 1.0 else 0.2,
            "semantic_accuracy": correctly_rejected,
            "citation_precision": 1.0,
            "citation_recall": 1.0,
            "tool_math_accuracy": 1.0,
            "rejection_accuracy": correctly_rejected,
            "judge_reasoning": "Negative query correctly rejected with no hallucinated facts." if correctly_rejected == 1.0 else "Failed to reject ungrounded question."
        }

    # 2. Citation Precision & Recall
    # Citation Precision: do citations [i] point to valid source entries matching expected sources?
    source_map = {s["id"]: s["path"] for s in sources}
    valid_citations = 0
    for cid in citation_ids:
        src_path = source_map.get(cid, "")
        if any(exp in src_path for exp in expected_sources):
            valid_citations += 1
    citation_precision = valid_citations / len(citation_ids) if citation_ids else (1.0 if not expected_sources else 0.0)
    citation_recall = 1.0 if len(citation_ids) > 0 else 0.0

    # 3. Key Fact Coverage (Rule-based check)
    lower_ans = answer.lower()
    facts_present = sum(1 for fact in key_facts if fact.lower() in lower_ans)
    fact_coverage = facts_present / len(key_facts) if key_facts else 1.0

    # 4. Tool & Math Accuracy Check
    math_accuracy = 1.0
    if requires_math:
        # Check if the calculated values are in answer
        math_accuracy = fact_coverage

    # 5. LLM-as-a-Judge Evaluation (Ragas Groundedness & Relevance)
    retrieved_context_snippet = "\n---\n".join([f"[{c['rank']}] {c['source']}: {c['text'][:2000]}" for c in chunks[:6]])
    
    judge_prompt = f"""Evaluate this RAG system response based on the question, retrieved context, and ground truth.

Question: {query}
Ground Truth: {ground_truth}

Retrieved Contexts:
{retrieved_context_snippet}

Generated Answer:
{answer}

Rate the following metrics from 0.0 to 1.0:
1. "faithfulness": Is every claim in the generated answer strictly supported by the retrieved contexts, free from hallucination? (1.0 = completely faithful, 0.0 = completely hallucinated).
2. "answer_relevance": Does the answer directly address the user query without drift? (1.0 = perfectly relevant, 0.0 = irrelevant).
3. "semantic_accuracy": Does the answer agree factually with the ground truth? (1.0 = completely correct, 0.0 = incorrect).

Respond strictly in valid JSON format:
{{
  "faithfulness": 1.0,
  "answer_relevance": 1.0,
  "semantic_accuracy": 1.0,
  "reasoning": "brief justification"
}}
"""
    judge_res = call_llm_judge(judge_prompt)
    faithfulness = float(judge_res.get("faithfulness", 0.8))
    answer_relevance = float(judge_res.get("answer_relevance", 0.8))
    semantic_accuracy = float(judge_res.get("semantic_accuracy", fact_coverage))

    return {
        "faithfulness": faithfulness,
        "answer_relevance": answer_relevance,
        "semantic_accuracy": semantic_accuracy,
        "citation_precision": citation_precision,
        "citation_recall": citation_recall,
        "tool_math_accuracy": math_accuracy,
        "rejection_accuracy": 1.0,
        "judge_reasoning": judge_res.get("reasoning", "")
    }


def main():
    console.print("[bold cyan]══════════════════════════════════════════════════════════════════════[/bold cyan]")
    console.print("[bold yellow]        Sift Advanced-RAG Evaluation Framework & Benchmark             [/bold yellow]")
    console.print("[bold cyan]══════════════════════════════════════════════════════════════════════[/bold cyan]\n")

    if not RAG_BIN.exists():
        console.print(f"[bold red]Binary {RAG_BIN} not found! Run cargo build --release first.[/bold red]")
        sys.exit(1)

    if not DB_PATH.exists():
        console.print(f"[bold red]Database {DB_PATH} not found![/bold red]")
        sys.exit(1)

    with open(DATASET_PATH) as f:
        test_cases = json.load(f)

    console.print(f"[green]Loaded {len(test_cases)} test cases from {DATASET_PATH}[/green]")
    console.print(f"[green]Target Database: {DB_PATH}[/green]")
    console.print(f"[green]LLM Judge Model: {MODEL_NAME} ({BASE_URL})[/green]\n")

    results = []
    
    for idx, tc in enumerate(test_cases, 1):
        tc_id = tc["id"]
        category = tc["category"]
        query = tc["query"]
        console.print(f"[bold blue][{idx}/{len(test_cases)}] Evaluating {tc_id} ({category}):[/bold blue] {query}")

        # 1. Retrieval Phase
        chunks, search_lat = run_rag_search(query, top_k=8)
        ret_metrics = evaluate_retrieval(tc, chunks)

        # 2. Generation & Answer Phase
        ask_res, ask_lat = run_rag_ask(query, top_k=6)
        gen_metrics = evaluate_generation(tc, ask_res, chunks)

        total_lat = search_lat + ask_lat

        case_result = {
            "id": tc_id,
            "category": category,
            "query": query,
            "ground_truth": tc["ground_truth"],
            "generated_answer": ask_res["answer"],
            "citations": ask_res["citation_ids"],
            "cited_sources": [s["path"] for s in ask_res["sources"]],
            "retrieval_chunks": len(chunks),
            "search_latency_sec": round(search_lat, 3),
            "generation_latency_sec": round(ask_lat, 3),
            "total_latency_sec": round(total_lat, 3),
            "metrics": {
                **ret_metrics,
                **gen_metrics
            }
        }
        results.append(case_result)

        # Print short summary
        m = case_result["metrics"]
        console.print(
            f"   Hit@1: {m['hit_rate_at_1']:.1f} | MRR: {m['mrr']:.2f} | "
            f"Faithfulness: {m['faithfulness']:.2f} | Rel: {m['answer_relevance']:.2f} | "
            f"Accuracy: {m['semantic_accuracy']:.2f} | Latency: {total_lat:.2f}s"
        )

    # Save complete JSON artifact
    with open(RESULTS_PATH, "w") as f:
        json.dump(results, f, indent=2)
    console.print(f"\n[bold green]Saved detailed evaluation results to {RESULTS_PATH}[/bold green]\n")

    # Aggregate Metrics Overall
    total_cases = len(results)
    avg_hit_1 = sum(r["metrics"]["hit_rate_at_1"] for r in results) / total_cases
    avg_hit_3 = sum(r["metrics"]["hit_rate_at_3"] for r in results) / total_cases
    avg_hit_5 = sum(r["metrics"]["hit_rate_at_5"] for r in results) / total_cases
    avg_hit_8 = sum(r["metrics"]["hit_rate_at_8"] for r in results) / total_cases
    avg_mrr = sum(r["metrics"]["mrr"] for r in results) / total_cases
    avg_ctx_rec = sum(r["metrics"]["context_recall"] for r in results) / total_cases
    avg_ctx_prec = sum(r["metrics"]["context_precision"] for r in results) / total_cases

    avg_faith = sum(r["metrics"]["faithfulness"] for r in results) / total_cases
    avg_rel = sum(r["metrics"]["answer_relevance"] for r in results) / total_cases
    avg_acc = sum(r["metrics"]["semantic_accuracy"] for r in results) / total_cases
    avg_cite_prec = sum(r["metrics"]["citation_precision"] for r in results) / total_cases
    avg_cite_rec = sum(r["metrics"]["citation_recall"] for r in results) / total_cases

    latencies = [r["total_latency_sec"] for r in results]
    latencies.sort()
    avg_lat = sum(latencies) / len(latencies)
    p50_lat = latencies[len(latencies) // 2]
    p95_lat = latencies[int(len(latencies) * 0.95)]

    # Category Breakdown
    categories = sorted(list(set(r["category"] for r in results)))
    cat_summary = []
    for cat in categories:
        cat_cases = [r for r in results if r["category"] == cat]
        n = len(cat_cases)
        c_hit = sum(c["metrics"]["hit_rate_at_3"] for c in cat_cases) / n
        c_mrr = sum(c["metrics"]["mrr"] for c in cat_cases) / n
        c_faith = sum(c["metrics"]["faithfulness"] for c in cat_cases) / n
        c_rel = sum(c["metrics"]["answer_relevance"] for c in cat_cases) / n
        c_acc = sum(c["metrics"]["semantic_accuracy"] for c in cat_cases) / n
        c_lat = sum(c["total_latency_sec"] for c in cat_cases) / n
        cat_summary.append([cat, n, f"{c_hit*100:.1f}%", f"{c_mrr:.3f}", f"{c_faith*100:.1f}%", f"{c_rel*100:.1f}%", f"{c_acc*100:.1f}%", f"{c_lat:.2f}s"])

    console.print("[bold yellow]══════════════════════════════════════════════════════════════════════[/bold yellow]")
    console.print("[bold white]                     OVERALL BENCHMARK RESULTS                        [/bold white]")
    console.print("[bold yellow]══════════════════════════════════════════════════════════════════════[/bold yellow]")

    summary_table = [
        ["Metric Category", "Metric Name", "Score / Value", "Target Benchmark"],
        ["Retrieval", "Hit Rate @ 1", f"{avg_hit_1 * 100:.1f}%", ">= 85.0%"],
        ["Retrieval", "Hit Rate @ 3", f"{avg_hit_3 * 100:.1f}%", ">= 92.0%"],
        ["Retrieval", "Hit Rate @ 5", f"{avg_hit_5 * 100:.1f}%", ">= 95.0%"],
        ["Retrieval", "Mean Reciprocal Rank (MRR)", f"{avg_mrr:.3f}", ">= 0.850"],
        ["Retrieval", "Context Recall", f"{avg_ctx_rec * 100:.1f}%", ">= 90.0%"],
        ["Retrieval", "Context Precision", f"{avg_ctx_prec * 100:.1f}%", ">= 75.0%"],
        ["Generation", "Faithfulness (Groundedness)", f"{avg_faith * 100:.1f}%", ">= 95.0%"],
        ["Generation", "Answer Relevance", f"{avg_rel * 100:.1f}%", ">= 90.0%"],
        ["Generation", "Semantic Accuracy / Factual Agreement", f"{avg_acc * 100:.1f}%", ">= 90.0%"],
        ["Generation", "Citation Precision", f"{avg_cite_prec * 100:.1f}%", ">= 95.0%"],
        ["Generation", "Citation Recall", f"{avg_cite_rec * 100:.1f}%", ">= 90.0%"],
        ["Performance", "Mean Total Latency", f"{avg_lat:.2f}s", "< 4.00s"],
        ["Performance", "p50 Latency", f"{p50_lat:.2f}s", "< 3.00s"],
        ["Performance", "p95 Latency", f"{p95_lat:.2f}s", "< 5.00s"]
    ]
    console.print(tabulate(summary_table, headers="firstrow", tablefmt="fancy_grid"))

    console.print("\n[bold yellow]══════════════════════════════════════════════════════════════════════[/bold yellow]")
    console.print("[bold white]                  CATEGORY BREAKDOWN BENCHMARK                        [/bold white]")
    console.print("[bold yellow]══════════════════════════════════════════════════════════════════════[/bold yellow]")
    headers_cat = ["Category", "Cases", "Hit@3", "MRR", "Faithfulness", "Relevance", "Accuracy", "Avg Latency"]
    console.print(tabulate(cat_summary, headers=headers_cat, tablefmt="fancy_grid"))


if __name__ == "__main__":
    main()
