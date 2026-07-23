# Repository Guidelines

## 项目目标
这个仓库正在实现一个基于 Rust 的 LLM 网关。第一阶段需要稳定支持 `completions`、`responses`、`messages` 三种协议。整体系统还需要支持多供应商、多后端、多模型、模型别名、供应商侧模型名映射、按模型定价、API Key 隔离、负载均衡、粘性会话、持久化存储，以及基于 SQLite 等轻量级嵌入式数据库的数据分析能力。

## 项目结构与模块划分
保持传输层和领域逻辑边界清晰。`src/protocol/` 负责协议处理，`src/backend/` 负责供应商适配，`src/routing/` 负责路由和粘性会话，`src/middleware/` 负责鉴权和限流，`src/server/` 负责共享状态和路由装配，`src/stats/` 负责用量与成本统计。运行时配置放在 `~/.rcpa/config.yaml`。集成测试放在 `tests/`。后续数据库能力建议单独放在 `src/store/` 或 `src/db/`。

## 构建、测试与开发命令
- `cargo run`：本地启动网关；配置缺失时会自动创建于 `~/.rcpa/config.yaml`。
- `cargo check`：快速做编译检查。
- `cargo test`：运行单元测试和集成测试。
- `cargo fmt`：应用 Rust 标准格式化。
- `cargo fmt --check`：检查格式是否符合 CI 要求。
- `cargo clippy --all-targets --all-features -- -D warnings`：要求 lint 零警告。

## 编码规范与命名约定
使用 `rustfmt` 默认风格和 4 空格缩进。模块、函数、配置字段使用 `snake_case`，类型使用 `UpperCamelCase`，常量使用 `SCREAMING_SNAKE_CASE`。HTTP handler 保持轻量，路由策略、鉴权策略、价格计算和数据库访问逻辑应放在独立模块中，不要堆进路由函数。

## 数据库与分析约定
优先支持 SQLite，用于本地开发、测试和轻量部署。存储层设计要留出扩展空间，后续可以接入其他嵌入式数据库而不改 handler 逻辑。需要持久化的数据至少包括用户、API Key、后端定义、模型策略、请求日志、Token 用量和成本记录。每次 schema 变更都要带 migration。分析能力应基于持久化数据查询，而不是只依赖内存统计。

## 迁移规则
不允许在运行时为旧配置、旧协议别名、旧数据库 schema 或历史脏数据保留兜底逻辑。数据库结构不匹配时，必须通过明确的 version-based migration 迁移数据和 schema；迁移系统不得依赖 table/column 探测、默认值回填、静默修复或自动删除业务表来绕过不匹配。schema 不匹配、migration 缺失或 migration 顺序异常时，启动必须直接失败并给出清晰错误。

## 模型名称与别名规则
供应商模型的 `name` 是发给供应商的真实模型名；供应商模型的 `aliases` 是平台公开模型名。有 `aliases` 时，API 和下拉选择只暴露这些公开名，不再同时暴露真实模型名；没有 `aliases` 时，真实模型名就是平台公开模型名。API Key 的 `model_aliases` 只表示 Key 私有别名，目标必须是一个已经存在的平台公开模型名，且 Key 私有别名不得和任何平台公开模型名重名。

## 测试要求
每个模块优先补充就近单元测试，跨模块行为放到 `tests/` 做集成测试。凡是修改协议映射、后端路由、Key 授权、粘性会话、定价逻辑或数据库 schema，都必须补测试。SQLite 相关能力需要提供可重复执行的集成测试夹具。

## 安全与配置要求
新增配置项必须在启动时校验。凡是依赖持久化的功能，都要明确数据库文件缺失、SQLite 锁冲突、migration 不匹配时的失败行为。
