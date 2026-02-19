# Petool 后端服务

AI 请求中转 + 用户账户管理 + 支付接口，基于 **FastAPI + MongoDB**。

## 环境要求

- Python 3.11+
- [uv](https://docs.astral.sh/uv/) — Python 包和项目管理工具
- MongoDB（本地安装或 MongoDB Atlas）

## 安装 uv

```bash
# Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"

# macOS / Linux
curl -LsSf https://astral.sh/uv/install.sh | sh
```

## 快速开始

```bash
# 进入 backend 目录
cd backend

# 创建虚拟环境并安装所有依赖（一步完成）
uv sync

# 复制并编辑环境变量
copy .env.example .env
# 编辑 .env，至少填写：
#   GLM_API_KEY  — 或其他 LLM 的 API Key
#   MONGODB_URL  — 默认 mongodb://localhost:27017

# 启动开发服务器
uv run uvicorn main:app --reload --port 8000
```

访问 [http://localhost:8000/docs](http://localhost:8000/docs) 查看自动生成的接口文档。

## 目录结构

```
backend/
├── pyproject.toml       # uv 项目配置 + 依赖声明
├── requirements.txt     # 兼容传统 pip（与 pyproject.toml 保持同步）
├── main.py              # FastAPI 应用入口
├── database.py          # MongoDB 连接管理
├── schemas.py           # 数据模型（Pydantic + MongoDB 文档结构）
├── auth_utils.py        # JWT 工具函数
├── routers/
│   ├── auth.py          # 注册 / 登录
│   ├── account.py       # 资料 / 余额 / 明细 / 订单
│   ├── proxy.py         # AI 请求中转（核心）
│   └── payment.py       # 微信支付 / 支付宝
├── certs/               # 支付证书目录（不提交 git）
├── .env                 # 本地环境变量（不提交 git）
├── .env.example         # 环境变量示例
└── requirements.txt     # Python 依赖
```

## 主要接口

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/auth/register` | 用户注册（赠 5 万 token） |
| POST | `/auth/login` | 登录，返回 JWT |
| GET  | `/account/profile` | 个人资料 |
| GET  | `/account/quota` | 余额 + 7 天趋势 |
| GET  | `/account/usage` | 消费明细（分页） |
| GET  | `/account/orders` | 订单列表 |
| POST | `/v1/chat/completions` | **AI 中转**（兼容 OpenAI 格式） |
| POST | `/payment/wechat/create` | 创建微信支付订单 |
| POST | `/payment/alipay/create` | 创建支付宝订单 |

## 环境变量说明

| 变量 | 说明 |
|------|------|
| `MONGODB_URL` | MongoDB 连接串，默认 `mongodb://localhost:27017` |
| `JWT_SECRET_KEY` | JWT 签名密钥，**生产环境务必修改** |
| `GLM_API_KEY` | 智谱 API Key |
| `ARK_API_KEY` | 字节 Ark API Key |
| `OPENAI_API_KEY` | OpenAI API Key |
| `WECHAT_PAY_*` | 微信支付配置（未填写时走 Mock 模式） |
| `ALIPAY_*` | 支付宝配置（未填写时走 Mock 模式） |

## 生产部署

```bash
# 安装依赖
uv sync

# 多进程启动（根据 CPU 核心数调整 workers）
uv run uvicorn main:app --host 0.0.0.0 --port 8000 --workers 4
```

> **生产前必做**：
> 1. `JWT_SECRET_KEY` 改为强随机字符串
> 2. 删除 `main.py` CORS 中的 `"*"`，改为具体域名
> 3. 将客户端 `petool_api_base` 改为服务器公网地址
