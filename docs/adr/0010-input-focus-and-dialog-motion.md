# 输入框聚焦视觉与弹窗动效统一

面板里的文本输入此前沿用 kit 默认焦点样式：3px 外扩焦点环叠加描边，在浅色卡片上显得粗重；笔记搜索框曾单独关闭焦点环、改用 1px 主色描边，但待办表单、分类标签弹窗与分类管理页的输入框没有跟上，聚焦反馈在不同输入之间不一致。弹窗动效同样分裂：kit Dialog 默认 `SlideDown`（自窗口顶部落下），笔记页的分类标签弹窗是自绘遮罩加卡片，使用 200ms、`ease_out_cubic`、自下方 6px 淡入；待办表单则没有居中。这些差异让同一应用内的表单行为看起来像两套实现。

定案一：主题层关闭 kit 焦点环（`Theme::focus_ring = false`），文本输入的聚焦反馈统一为 1px 主色半透明描边。新模块 `crates/fragmenta/src/ui/focus.rs` 提供 `input_border(theme, focused)`（主色 0.6 不透明度 / `theme.input`）与 `TextFieldExt::text_field_focus`：`Input` 同时关闭自身焦点环，`TimeField` 直接改描边色。笔记搜索框、待办内容与起止时间、分类标签弹窗的分类与标签输入、分类管理页的新建 / 重命名 / 十六进制输入全部经该入口着色，页面不再各写不透明度常量。非文本可聚焦控件（按钮、勾选框）关闭焦点环后退回 kit 的 1px `theme.ring` 描边或内嵌细线，不再有外扩阴影，接受这一轻微差异。

定案二：弹窗动效统一取主题 motion token：`duration_normal`、`easing_enter`、`distance_short`（0.25rem），方向统一为自上方略微下落入场、反向升出退场；遮罩与卡片同用 `theme.overlay` 与同一时长淡入淡出。`duration_normal` 在主题应用时统一改写为 280ms（kit 默认 180ms 观感偏快，且两种方向共用该 token，故取参考退场时长 0.28s）。新增模块 `crates/fragmenta/src/ui/dialog_motion.rs` 作为唯一入口：`kit_dialog()` 为 kit Dialog 套用 `DialogEntrance::FadeSlide`，`alert_dialog()` 为确认弹窗（笔记删除、分类 / 标签删除）套用同一入场，`DialogMotion` 为自绘弹窗提供 `card()` / `backdrop()` 两套 `EffectTransition` 与关闭计时。待办新建 / 编辑表单、退出失败提示、删除确认与笔记页分类标签弹窗全部经此模块配置；减少动态效果时都退化为即时呈现。GPUI 没有 CSS 的 `filter: blur()`，动效以淡入淡出加短距位移表达，曲率与时长以主题 token 为准而不是逐处手写秒数。

定案三：表单弹窗的 Enter 语义：勾选框聚焦时只切换勾选，其余焦点位置等价于点击「保存」（内容为空时保存不生效），Esc、遮罩点击与「取消」仍按 kit 默认语义关闭。kit Dialog 把 `enter` 绑定为 `Confirm`，并在按键按下阶段就吞掉事件，因此聚焦元素的原生 Enter 激活不会发生。实现方式：勾选框外层容器注册就近的 `Confirm` 处理器（bubble 阶段先于弹窗宿主）自行翻转勾选并截断传播；弹窗 `on_ok` 则调用与「保存」按钮相同的提交函数并返回 `false`，由提交逻辑决定是否关闭（成功才关闭，失败或内容为空保持打开）。

代价与约束：文本输入必须显式经 `ui/focus` 着色，否则聚焦时只有主题回退的 ring 色描边；新增 Dialog 应使用 `dialog_motion::kit_dialog()`，确认弹窗使用 `alert_dialog()`，自绘弹窗复用 `DialogMotion`；普通 `Dialog` 不渲染默认按钮，表单 footer 仍需自行组装。验收以人工为主：在浅色与深色主题下检查各处输入框聚焦观感一致、四类弹窗动画一致，并验证 Tab 到勾选框 / 保存按钮后 Enter 的勾选与保存行为。

状态：accepted（2026-10-29）
