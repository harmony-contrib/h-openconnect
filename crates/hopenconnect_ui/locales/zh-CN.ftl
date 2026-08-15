# H-OpenConnect 中文 (zh-CN) catalog — Fluent

# --- Navigation / status (was UiStrings) ---
nav-home = 连接
nav-connections = 配置
nav-statistics = 统计
nav-more = 更多
home-title = 安全连接
connect = 连接
disconnect = 断开
connecting = 正在连接…
disconnecting = 正在断开…
connected = 已连接
disconnected = 未连接
failed = 连接失败
select-connection = 选择连接
no-connection = 尚未选择连接
current = 当前
server = 服务器地址
group = 隧道组 / Group
protocol = 主协议
username = 用户名
password = 密码
name = 连接名称
auth-method = 认证方式
certificate = 证书
backup-servers = 备用服务器
basic = 基本信息
strict-cert = 严格证书校验
block-untrusted = 阻止不受信任的服务器
local-lan = 允许本地局域网
force-global = 强制全局路由
force-global-desc = 忽略服务器分流，全部 IPv4 流量走 VPN
connect-on-demand = 意外断线后自动重连
external-browser = 外部浏览器认证 (SAML)
fips-mode = FIPS 模式
mtu-override = MTU 覆盖
cancel = 取消
add-connection = 添加连接
edit-connection = 编辑连接
favorite = 收藏
appearance = 外观
diagnostics = 日志
logs-search-placeholder = 输入关键词、等级或时间
logs-empty-title = 暂无日志
logs-empty-subtitle = 核心和 VPN 事件会显示在这里。
logs-level-all = 全部
about = 关于
language = 语言
theme = 主题
system = 跟随系统
light = 浅色
dark = 深色
assigned-ip = 分配地址
duration = 连接时长
sent = 发送
received = 接收
gateway = 网关
mtu = MTU
packets-sent = 发送包
packets-received = 接收包
version = 版本
sdk-status = 协议栈
sdk-pending = 未链接
sdk-ready = OpenConnect 已链接
empty-connections = 还没有连接配置，点右上角添加。
form-required = 连接名称与服务器地址不能为空
feedback-connected = 已建立 VPN 连接
feedback-disconnected = 连接已断开
feedback-failed = 连接失败，请查看错误信息与诊断日志
feedback-deleted = 连接配置已删除
auth-password = 密码
auth-certificate = 证书
auth-password-cert = 密码+证书
auth-saml = SAML
mtu-auto = 自动
challenge-submit = 继续

# --- Lifecycle ---
lifecycle-authenticating = 正在认证…
lifecycle-establishing = 正在建立隧道…

# --- Toasts (state.rs / tasks.rs) ---
toast-open-browser-failed = 无法打开浏览器
toast-no-auth-form = 当前没有待处理的认证表单
toast-enter-otp = 请填写动态口令/验证码后再点继续
toast-group-fallback = 原分组不在服务器列表中，已切换到服务器默认分组
toast-group-fetch-failed = 未能自动获取分组，可手动填写
toast-open-link-failed-prefix = 无法打开链接：
toast-log-started = 已开始记录日志
toast-log-stopped = 已停止记录日志
toast-log-toggle-failed-prefix = 切换日志记录失败：
toast-log-exported-prefix = 日志已导出：
toast-log-export-failed-prefix = 日志导出失败：
toast-log-deleted-prefix = 日志已删除：
toast-log-delete-failed-prefix = 日志删除失败：
toast-enter-password = 请先填写密码
toast-select-cert = 请先选择客户端证书文件

# --- About ---
about-not-linked = 未链接
about-tagline = HarmonyOS 安全远程接入客户端
about-application = 应用信息
about-open-source = 开源与许可
about-privacy = 隐私
about-privacy-storage = 连接配置与凭据保存在应用私有目录，并排除在系统备份之外。
about-privacy-logs = 诊断日志默认关闭，仅在主动开启后写入本地按日归档。
about-privacy-no-telemetry = 应用不包含分析或遥测上传；网络请求由你配置的 VPN 与认证流程触发。
about-disclaimer = H-OpenConnect 是独立开源项目，与 Cisco 无隶属或背书关系；相关名称与商标归其各自所有者。

# --- Home ---
home-live-session = 真实 AnyConnect / OpenConnect 会话
home-not-linked = 当前构建未链接 OpenConnect

# --- Challenge ---
challenge-required = 服务器需要额外认证
challenge-round = 第 { $n } 轮认证表单
challenge-placeholder = 在此输入

# --- Statistics ---
statistics-hint = 连接成功后显示真实流量与分配地址

# --- Settings ---
settings-preferences = 偏好
settings-language-theme = 语言与浅色 / 深色主题
settings-operations = 运维
settings-logs = 查看 OpenConnect 运行日志
settings-about = 开源信息、组件版本与隐私说明
settings-language-hint = 选择界面语言；跟随系统会响应系统语言变化
settings-theme-hint = 切换浅色、深色或跟随系统；修改会立即生效

# --- Connections editor ---
conn-fetching-groups = 正在获取服务器分组…
conn-reading-groups = 正在读取 AnyConnect 认证分组
conn-browse = 选择文件
conn-saml-login = 系统浏览器完成 SAML 登录
conn-pin-top = 在列表中优先展示
conn-advanced = 高级配置
conn-advanced-desc = 关闭时使用默认值，不展示非常用选项
conn-show-advanced = 显示高级选项
conn-advanced-detail = 协议、证书细节、代理、分流与令牌等
conn-cert-path-placeholder = 客户端证书路径 PEM/P12
conn-private-key-path = 私钥路径（可选）
conn-key-password = 证书口令 (PKCS#12/PEM)
conn-secondary-cert = 第二客户端证书（MCA，可选）
conn-secondary-key = 第二证书私钥（可选）
conn-secondary-password = 第二证书口令
conn-software-token = 软件令牌
conn-token-string = 令牌字符串
conn-ca-cert-path = CA 证书路径
conn-split-mode = 分流模式
conn-split-networks = 自定义分流网段
conn-reported-os = 上报 OS
conn-client-version = 客户端版本
conn-http-proxy = HTTP 代理
conn-cert-pin = 服务器证书钉扎
conn-trusted-apps = 信任应用包名
conn-blocked-apps = 排除应用包名
conn-dtls = 启用 DTLS 数据通道（推荐）
conn-pfs = 要求完美前向保密
conn-no-xml-post = 禁用 XML POST（少数网关需要）
conn-reject-mismatch = 拒绝主机名不匹配或不完整证书链
conn-abort-untrusted = 服务器不受信任时中止连接
conn-allow-insecure = 允许不安全加密
conn-allow-insecure-desc = 仅用于必须使用 3DES/RC4/SHA1 的旧网关；与证书信任无关
conn-local-lan = VPN 连接期间仍可访问本地网络
conn-auto-connect = 网络可用时自动建立隧道
conn-fips-unavailable = 当前运行时不提供已认证的 FIPS Provider

# --- Logs ---
logs-current-tab = 当前日志
logs-history-tab = 历史记录
logs-recording-active-detail = 正在写入，停止记录后可删除
logs-recording-on = 正在记录并按天保存
logs-recording-off = 日志记录已关闭
logs-files-suffix = 个日志文件
logs-no-history = 暂无历史日志
logs-no-history-desc = 开启日志记录后会按天生成文件
logs-count-suffix = 条日志
logs-tap-detail = 点击日志查看全文
logs-start-recording-hint = 点击右上角开始记录日志
logs-delete-title = 删除历史日志？
logs-delete-desc = 此操作无法撤销
logs-delete-action = 删除
logs-detail-title = 日志详情
