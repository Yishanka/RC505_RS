# REQUIREMENTS 路由化拆分说明

## 目标

将原先单份 `AGENTS/REQUIREMENTS.md` 拆分为：

1. 一个总入口
2. 一个共享规则文档
3. 三个按任务类型划分的专项要求文档

目的是降低后续每次任务都完整读取大文档的上下文开销。

## 新结构

1. `AGENTS/REQUIREMENTS.md`
   - 总入口
   - 路由规则
   - 永久硬约束
2. `AGENTS/requirements/10-global.md`
   - 所有任务共享规则
3. `AGENTS/requirements/20-feature-dev.md`
   - 功能开发要求
4. `AGENTS/requirements/30-change-debug.md`
   - bug 修复 / 功能修改 / 功能删除要求
5. `AGENTS/requirements/40-refactor.md`
   - 架构与风格类重构要求

## 为什么这样拆

原始单文档本身不算非常大，但如果要求“每次开发前都先读完整文件”，会产生稳定的重复 token 消耗。

拆分后的收益：

1. 仍保留统一入口，不会失去统一规范
2. 后续大多数任务只需读取“入口 + 共享规则 + 当前任务专项要求”
3. 功能开发、debug、重构三类任务互不干扰
4. 以后新增规范时可以继续挂到 `AGENTS/requirements/` 下，而不是持续膨胀主文件

## 使用方式

后续任何开发默认：

1. 先读 `AGENTS/REQUIREMENTS.md`
2. 再读 `AGENTS/requirements/10-global.md`
3. 再按任务类型读专项要求

如果是混合型任务，则读取多个专项要求。

## 验证方法

1. 检查入口文档是否仍明确要求“先读 `AGENTS/REQUIREMENTS.md`”
2. 检查入口文档是否提供了任务分类路由
3. 检查三个专项文档是否分别覆盖功能开发、问题修改、重构
4. 检查入口文档是否仍保留“默认不修改 `AGENTS/PLAN.md`”约束

## 风险与限制

1. 如果未来专项文档之间出现重复规则，需要定期回收并归并到 `10-global.md`
2. 如果某次任务跨多个类别，模型需要主动读取多个专项文档，而不能只读一个
