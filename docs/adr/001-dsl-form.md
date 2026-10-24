# ADR-001: DSL 形态——YAML 主干 + CEL 谓词，Python 仅限 L4 逃逸舱

- 状态：已裁决（2026-10-24，编排者）
- 取代：使命指令 v0.1 草案中"YAML 主干 + Python 内嵌表达式"的表述（Python 降级为逃逸舱语义）

## 背景

1. 拍板版（设计/产品设计_拍板版.md §2.3 判据 8 / D3）已裁决：配方 DSL 必须是**类型化受限 DSL——无图灵完备、无副作用、可静态校验**。
2. poc-cue 实测（verify-load/REPORT.md）：CUE 的 or() 引用完整性析取在千条规则下近平方爆炸（vet 88.9s vs 线性 1.06s，差 47–84 倍），已裁决"引用完整性出 CUE 进装载器"；rhai 等图灵完备脚本被设计纪律明令排除。
3. af-graph 已集成 cel-interpreter 0.10（`src/predicate.rs`，含 catch_unwind 兜底畸形串 panic），CEL 是工作区内已验证的受限谓词载体。

## 裁决

1. DSL 语法主干 = **YAML**（serde 解析为 AST，deny_unknown_fields 拒绝式信封）。
2. 一切运行时谓词（停止条件、门控条件、选择器规则）= **CEL 表达式**，编译期 compile、无副作用。
3. **Python 不进入 L0–L3 任何语法层**；仅存在于 L4 逃逸舱（escape_hatches），且：必须显式声明、必须在失败分类账记账、健康度指标中逃逸舱占比 >30% 警戒。
4. 结构校验由 Rust 装载器承担（三层错误分类学：E0 信封 / E1 形状 / E2 语义 / E9 部署期，沿用 poc-cue loader 范式）；不引入 CUE 作为用户面语法。

## 后果

- 编译器前端 = YAML→AST（serde）+ CEL compile；IR 是第一接口（宪法 C5），语法可换、IR 慎改。
- 使命指令中"Python 内嵌逃逸舱"的定位与本裁决一致：逃逸是零语法代价的最后手段，不是常规表达方式。
