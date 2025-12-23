# Module Analysis Agent

Analyze the Rust module at path: $ARGUMENTS

## Analysis Tasks

Perform a comprehensive code review with the following structure:

### 1. Module Structure & Code Volume

- List all files in the module with line counts
- Identify main structs, traits, and functions with their responsibilities
- Create a summary table: `| File | Lines | Main Purpose |`

### 2. Function-by-Function Analysis

For each significant function/method (>20 lines), evaluate:

- **Line count**
- **Quality rating** (1-4 stars):
  - ⭐ Poor: Hard to understand, many issues
  - ⭐⭐ Fair: Works but has significant problems
  - ⭐⭐⭐ Good: Well-structured, minor issues
  - ⭐⭐⭐⭐ Excellent: Clean, well-documented, idiomatic Rust
- **Brief description** of what it does
- **Issues** (if any): hardcoding, duplication, complexity, ownership issues

### 3. Crate Extraction Candidates

Identify code that could be extracted into a separate crate or made more reusable:

#### ✅ Recommended for Extraction

| Code | Location | Reason | Suggested Crate/Module |
| ---- | -------- | ------ | ---------------------- |

#### ⚠️ Consider for Generalization (Optional)

| Code | Location | Reason |
| ---- | -------- | ------ |

### 4. Code Quality Issues

#### 🔴 Critical Issues

- Code duplication (list specific functions/patterns)
- Hardcoded values that should be configurable
- Security concerns (unsafe blocks, unchecked inputs)
- Potential panics (unwrap/expect on user data)

#### 🟡 Moderate Issues

- Functions with too many responsibilities
- Missing or incomplete error handling
- Non-idiomatic Rust patterns
- Clippy warnings

#### 🟢 Minor Issues

- Style inconsistencies
- Missing documentation (public API docs)
- Naming conventions

### 5. Duplicate Code Detection

- Find functions/patterns that appear multiple times
- Calculate approximate duplication percentage
- Suggest consolidation strategies (traits, generics, macros)

### 6. Technical Debt Analysis

#### 🐢 Performance Bottleneck Concerns

Identify code where performance may be critical and potential bottlenecks exist:

- Unnecessary allocations (String instead of &str, Vec cloning)
- Excessive .clone() calls that could use references
- Inefficient async patterns (blocking in async context, unnecessary spawns)
- Missing opportunities for zero-copy parsing
- Suboptimal iterator usage (collect then iterate vs chaining)

| Location | Concern | Potential Impact | Suggestion |
| -------- | ------- | ---------------- | ---------- |

#### 🧠 Readability & Context Issues

Code that is hard for humans or LLMs to understand:

- Functions that are too long or do too many things
- Complex lifetime annotations that could be simplified
- Unclear variable/function names that require domain knowledge to decode
- Implicit dependencies or hidden state that makes control flow hard to follow
- Missing context: why does this code exist? What problem does it solve?
- Magic numbers or unexplained constants

| Location | Problem | Why It's Hard to Understand | Suggestion |
| -------- | ------- | --------------------------- | ---------- |

#### 🔗 Coupling & Generalization Opportunities

Functionality that is tangled together and could be separated or generalized:

- Business logic mixed with I/O code
- Protocol-specific code that could be reusable utilities
- Tight coupling between modules that should be independent
- Missed abstraction opportunities (traits, generics)

| Location | What's Coupled | Decoupling Benefit | Suggestion |
| -------- | -------------- | ------------------ | ---------- |

#### 🦀 Rust-Specific Concerns

- Unsafe code usage and safety documentation
- Error type design (thiserror, anyhow patterns)
- Feature flag organization
- Public API surface (what should be pub, pub(crate), private)
- Dependency bloat (unnecessary dependencies in Cargo.toml)

| Location | Concern | Suggestion |
| -------- | ------- | ---------- |

### 7. Summary Table

| Metric                    | Value               |
| ------------------------- | ------------------- |
| Total Lines               |                     |
| Number of Files           |                     |
| Crate Extraction Candidates |                   |
| Duplication Estimate      |                     |
| Overall Quality           |                     |
| Technical Debt Level      | Low / Medium / High |
| Refactoring Priority      |                     |

### 8. Recommended Actions

List concrete refactoring steps in priority order:

1. (Highest priority)
2. ...
3. (Lowest priority)

---

## Instructions for Agent

1. First, use Glob to find all Rust files (*.rs) in the target module
2. Read each file completely
3. Also read Cargo.toml to understand dependencies and features
4. Perform the analysis above in Korean (한국어)
5. Be specific with line numbers and code references
6. Focus on actionable insights, not generic advice
7. **For technical debt analysis, prioritize practical concerns over theoretical best practices**
8. **Pay special attention to async/streaming patterns as this is a streaming library**

---

## Post-Refactoring Report

After completing any refactoring work based on this analysis, always provide a summary of changes:

### Work Summary Template

```markdown
## 작업 효과 요약

### 변경된 파일

| 파일 | 변경 유형 | 설명 |
| ---- | --------- | ---- |

### 정량적 효과

| 지표               | 이전 | 이후 | 개선 |
| ------------------ | ---- | ---- | ---- |
| 코드 라인 수       |      |      |      |
| 중복 코드          |      |      |      |
| unwrap/expect 사용 |      |      |      |
| Clippy 경고        |      |      |      |

### 주요 개선 사항

- (구체적인 개선 내용)

### API 변경 사항

- (pub API 변경, 새로운 traits/structs, breaking changes 등)

### 테스트 결과

- `cargo test` 통과 여부
- `cargo clippy` 경고 여부
- `cargo doc` 빌드 여부
```

**Important**: Always share this summary with the user after completing refactoring tasks.
**Important**: Run `cargo check`, `cargo test`, and `cargo clippy` after refactoring to verify changes.
