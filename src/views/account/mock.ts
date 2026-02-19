export interface AccountProfile {
  name: string
  email: string
  plan: string
  level: string
  avatar: string
  tokensRemaining: number
  tokenUsagePercent: number
  expirationDate: string
  daysLeft: number
}

export interface RenewPlan {
  id: string
  name: string
  priceLabel: string
  priceUnit: string
  originalPrice?: string
  badge?: string
  featured?: boolean
  features: string[]
}

export interface TokenPack {
  id: string
  name: string
  desc: string
  priceLabel: string
  available: boolean
}

export interface OrderRecord {
  id: string
  title: string
  amount: string
  createdAt: string
  status: 'completed' | 'refunded'
}

export interface QuotaTrendPoint {
  date: string
  value: number
}

export interface QuotaUsageRecord {
  id: string
  createdAt: string
  taskType: string
  taskIcon: string
  taskIconClass: string
  model: string
  detailA: string
  detailB: string
  amount: number
  emphasize?: boolean
}

export const accountProfile: AccountProfile = {
  name: 'Alex Chen',
  email: 'alex.chen@example.com',
  plan: 'Pro Plan',
  level: 'LV.3',
  avatar:
    'https://lh3.googleusercontent.com/aida-public/AB6AXuBYaZM97JogdW-ya3ULqGOtiyNOHmX7QgQJQ1c7qMdDxTpN__9ZBn0Jq6D5AQiHwClbXSmKaP3yFa-GzJuTHIsZ6OObIjCQ9QHApIpAuKMYIWptOHH6KVzLGp4nU5DO48mIg48o3YedtwFShv6G0Tq-ir30SVT7WgAWCksaPf_PnwnEwCx7rOimt23ZlQC3VUyfRbucQrEvpTkLIEwEwiWZ_gSWFyekl4IxXUqKEUqrS2CVHHlvuJqUmCJBLBYKUuDKiuQqkueqB3Y',
  tokensRemaining: 12450,
  tokenUsagePercent: 75,
  expirationDate: '2024年12月31日',
  daysLeft: 186
}

export const renewPlans: RenewPlan[] = [
  {
    id: 'monthly',
    name: '月度会员',
    priceLabel: '¥29',
    priceUnit: '/ 月',
    features: ['文件夹投喂', '本地脚本运行', '无限次对话']
  },
  {
    id: 'yearly',
    name: '年度会员',
    priceLabel: '¥299',
    priceUnit: '/ 年',
    originalPrice: '¥348/年',
    badge: '推荐 · 8 折优惠',
    featured: true,
    features: ['包含所有月度权益', '优先体验新功能', '专属客服支持']
  }
]

export const tokenPacks: TokenPack[] = [
  {
    id: 'pack-100w',
    name: '100万 Token 加油包',
    desc: '不限时间，用完为止',
    priceLabel: '¥9.9',
    available: true
  },
  {
    id: 'pack-500w',
    name: '500万 Token 加油包',
    desc: '敬请期待',
    priceLabel: '--',
    available: false
  }
]

export const ordersSummary = {
  yearlyAmount: '¥ 348.00',
  monthAmount: '¥ 29.00'
}

export const orderRecords: OrderRecord[] = [
  {
    id: '#ORD-20240620-8832',
    title: '专业版 - 月度订阅',
    amount: '¥29.00',
    createdAt: '2024-06-20 14:30',
    status: 'completed'
  },
  {
    id: '#ORD-20240520-7721',
    title: '500k Tokens 充值包',
    amount: '¥18.00',
    createdAt: '2024-05-20 09:15',
    status: 'completed'
  },
  {
    id: '#ORD-20240515-6610',
    title: '专业版 - 年度订阅',
    amount: '¥299.00',
    createdAt: '2024-05-15 11:20',
    status: 'refunded'
  },
  {
    id: '#ORD-20240420-5599',
    title: '专业版 - 月度订阅',
    amount: '¥29.00',
    createdAt: '2024-04-20 14:30',
    status: 'completed'
  }
]

export const quotaDashboard = {
  totalBalance: '1,250,400',
  consumedToday: '12,500'
}

export const quotaTrend: QuotaTrendPoint[] = [
  { date: '06-15', value: 2100 },
  { date: '06-16', value: 4200 },
  { date: '06-17', value: 12500 },
  { date: '06-18', value: 5100 },
  { date: '06-19', value: 3800 },
  { date: '06-20', value: 6200 },
  { date: '06-21', value: 2900 }
]

export const quotaUsageRecords: QuotaUsageRecord[] = [
  {
    id: 'usage-1',
    createdAt: '2024-06-21 14:30',
    taskType: '/doc 策划案生成',
    taskIcon: 'description',
    taskIconClass: 'blue',
    model: 'GLM-4-Pro',
    detailA: 'In: 850',
    detailB: 'Out: 2,100',
    amount: 2950,
    emphasize: true
  },
  {
    id: 'usage-2',
    createdAt: '2024-06-21 13:15',
    taskType: '日常对话',
    taskIcon: 'chat',
    taskIconClass: 'purple',
    model: 'GLM-4-Air',
    detailA: 'In: 120',
    detailB: 'Out: 450',
    amount: 570
  },
  {
    id: 'usage-3',
    createdAt: '2024-06-21 11:42',
    taskType: '/img 插画绘制',
    taskIcon: 'image',
    taskIconClass: 'green',
    model: 'DALL-E-3',
    detailA: 'Size: 1024x1024',
    detailB: 'Count: 1',
    amount: 1000
  },
  {
    id: 'usage-4',
    createdAt: '2024-06-21 10:20',
    taskType: '/trans 文档翻译',
    taskIcon: 'translate',
    taskIconClass: 'orange',
    model: 'GPT-4o',
    detailA: 'In: 450',
    detailB: 'Out: 620',
    amount: 1070
  },
  {
    id: 'usage-5',
    createdAt: '2024-06-20 19:55',
    taskType: '日常对话',
    taskIcon: 'chat',
    taskIconClass: 'purple',
    model: 'GLM-4-Air',
    detailA: 'In: 55',
    detailB: 'Out: 120',
    amount: 175
  }
]
