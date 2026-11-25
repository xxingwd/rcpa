# 后台浏览器回归

先运行 `cargo build`，再运行 `node tests/admin_browser.cjs`。脚本需要可用的 Playwright 和 Chromium；可通过 `PLAYWRIGHT_MODULE` 指定 Playwright 模块路径，通过 `RCPA_BROWSER_PATH` 指定浏览器可执行文件。

脚本自动创建临时配置和 SQLite 数据库，启动本地网关与确定性上游，结束后关闭它们。不会连接用户运行中的服务。测试覆盖真实请求统计、当前模型筛选、多维度多选、供应商详情页草稿与批量添加、模型抽屉与焦点恢复、models.dev 批量导入，以及亮暗主题和响应式布局。models.dev 在浏览器网络边界提供固定数据，配置写入和分析查询使用真实后端。

布局回归覆盖仪表盘、日志、供应商、模型目录、密钥和配置编辑页，检查页面撑满可视区、内容区域内部滚动、固定标题及滚动边界。长内容通过浏览器内的临时占位元素模拟，不写入业务数据。

首页性能表直接展示各模型与供应商的请求量、RPM、完整成功率、错误数、首包 P50/P95、估算 TPS P50/P10、有效样本数及总耗时 P95/重试率。回归核对真实持久化统计、非流式请求不产生 TPS，以及近 5 分钟快捷范围与全局筛选同步；桌面 1280×720 等尺寸检查概览一屏展示、长列表仅在表内滚动。

提示框回归覆盖图标悬停、键盘聚焦与 Escape、动态列表按钮，以及三张图表在亮暗主题下的实际 hover；同时检查提示框打开时切换主题。

示例：

```bash
cargo build
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright \
RCPA_BROWSER_PATH=/path/to/chromium \
node tests/admin_browser.cjs
```

截图、数据库和服务日志位于脚本输出的临时目录。Rust 测试仍使用 `cargo test`。

首页慢查询回归使用 `node tests/dashboard_refresh_browser.cjs`，同样先运行 `cargo build` 并设置上述 Playwright／Chromium 路径。它启动隔离网关，控制响应和浏览器时钟，验证慢请求跨过多个刷新周期仍可完成、完成后恢复轮询，以及用户切换时间范围能取消旧请求。

`cargo test --test dashboard_migration_test` 使用临时 SQLite 文件验证统计索引升级、历史正文与元数据保留、更新后的性能统计和重复启动。`store::analytics` 的查询计划测试还检查实际 SELECT 的执行字节码，防止新增统计字段再次触发回表。首次应用 `20261115000000` migration 会扫描日志并重建统计索引，需要预留升级时间；迁移失败会返回启动错误，不会跳过迁移。

模型 Logo 可填写 HTTP(S) 图片地址，导入保存独立的 `logo_url`。浏览器回归还覆盖列表空白处点击与键盘 Enter、编辑后焦点恢复、Logo 保存及日志展示、日志详情分类与正文预览、流式消息解析、快速切换记录的过期响应隔离，以及详情在亮暗主题和窄屏下的内部滚动。图片服务在网络边界使用确定性彩色 SVG。

首页分析区不再折叠：模型性能、请求/延迟/Token趋势、Key与供应商用量、协议及状态码同时展示。布局回归检查这些区块在桌面可视区内完整出现，列表溢出仅在各自区域滚动；用量更新保留原有行节点。

供应商模型近期统计回归：`node tests/model_recent_browser.cjs`。沿用上面的 Playwright / Chromium 环境变量；使用独立数据库和上游夹具，覆盖供应商隔离、5 分钟范围、过期响应、无样本及失败状态、表单草稿和焦点保留，以及浅色 / 深色 / 手机布局。
