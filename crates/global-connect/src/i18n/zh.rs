//! Simplified Chinese.
//!
//! `zh` and not `zh-Hans` or `zh-CN`, matching the product's rule: the primary
//! subtag is what is negotiated, so a browser asking for zh-CN, zh-Hans or
//! zh-SG lands here. Traditional would be a genuinely different catalog.
//!
//! Chinese was the first translation deliberately. English, German and French
//! set roughly the same amount of ink on a line, so a layout that holds one
//! holds the others; Chinese sets far less for the same sentence and breaks
//! anywhere rather than at spaces. If the hero survives this, the rest is
//! translation rather than design.
//!
//! Two consequences visible in the markup rather than here: headings are
//! `text-balance` so a short line does not leave a stranded character, and
//! nothing on the site is sized by counting English words.

use super::{
    About, AppCopy, Beneath, Common, Contact, Footer, Home, Industry, Nav, NotFound, PillarCopy,
    PlanCopy, Pricing, Product, Solutions, Strings,
};

pub static STRINGS: Strings = Strings {
    code: "zh",

    common: Common {
        start_free: "免费开始",
        get_started: "开始使用",
        sign_in: "登录",
        talk_to_us: "联系我们",
        see_inside: "看看里面有什么",
        tagline: "让好用的管理软件，人人用得上。",
        skip_to_content: "跳到正文",
        menu: "菜单",
        language: "语言",
        home_of: "首页",
        shot_alt: "{product} 的{app}：{screen}",
    },

    nav: Nav {
        solutions: "解决方案",
        product: "产品",
        pricing: "价格",
        about: "关于",
        contact: "联系",
        main: "主导航",
    },

    footer: Footer {
        product: "产品",
        company: "公司",
        account: "账户",
        whats_inside: "包含什么",
        privacy: "隐私",
        terms: "条款",
        rights: "为每天要让账目对得上的人而做。",
    },

    home: Home {
        title: "让好用的管理软件，人人用得上",
        description: "会计、库存与人员，同在一个工作区里，按业务本来的样子建模。",
        eyebrow: "一个工作区，覆盖业务的每一部分。",
        headline_lead: "让好用的管理软件，",
        headline_accent: "人人用得上。",
        lede: "会计、库存与人员集中在一处 —— 按工作本来的样子建模，让小公司今天就能上手，也经得起长成大公司之后继续用。",
        trial_note: "免费试用 {days} 天。无需信用卡，工作区有自己的独立地址。",
        apps_title: "每一块都独立，只开你要的那几块。",
        apps_lede: "每个应用都是产品中独立的一块。哪天真正用得上，哪天再打开它，其余的先放着。",
        apps_more: "采购、销售、项目与薪酬正在路上，它们会作为新的一块加进来，而不是另一件要另外买的产品。",
        dense_title: "该密的地方，就要密。",
        dense_lede: "一天的工作是在表格和表单里度过的，不是在落地页上。屏幕就是为此而做：紧凑的行、以键盘为先，任何一个数字都不超过一眼的距离。",
        reasons_title: "照着工作的形状来做。",
        hand_note: "真的，开一个就这么快",
        cta_title: "开一个工作区，大约一分钟。",
        cta_body: "取个名字，选个地址，它就是你的了 —— 自己的数据库、自己的用户、自己的权限。不与任何人共用。",
    },

    product: Product {
        title: "包含什么",
        description: "会计、库存与人员 —— 一个工作区，只开你要的那几块。",
        eyebrow: "产品",
        headline: "一个工作区，只开你要的那几块。",
        lede: "下面每个应用都是独立的一块。它们共用同一套用户、同一套权限和同一本总账，而且任何一个不依赖其他就能用。",
        underneath: "而在这三者之下",
        beneath: [
            Beneath {
                heading: "你自己的数据库",
                body: "一个工作区，一个数据库，一个地址。不是共享表上的一个租户字段。",
            },
            Beneath {
                heading: "真正管用的权限",
                body: "每个页面、每个操作都有关卡，而关卡是在服务端校验的。",
            },
            Beneath {
                heading: "一条审计轨迹",
                body: "谁在什么时候改了什么。事情发生时就写下来，而不是事后再拼凑。",
            },
            Beneath {
                heading: "四种语言",
                body: "英语、德语、法语和中文，在构建时校验，保证彼此不脱节。",
            },
        ],
        cta_title: "用你自己的数据看一看。",
        cta_body: "开一个工作区大约一分钟，而且开了就是你的。",
    },

    pricing: Pricing {
        title: "价格",
        description: "免费开始；当它成为你经营业务的方式时，再按人付费。",
        eyebrow: "价格",
        headline: "“人人用得上”，价格也算数。",
        lede: "免费开始，规模还小的时候就一直免费。为真正在用它的人付费，而不是为你打开的模块付费。",
        most: "多数公司的选择",
        trial_note: "每个方案都先有 {days} 天完整产品试用，无需信用卡。",
        provisional_lead: "这些数字尚未确定。",
        provisional_body: "方案的结构是我们正在搭建的样子；价格仍在敲定，会在向任何人收费之前先公布在这里。",
        hand_note: "无需信用卡，也没有什么要取消的",
        faq_title: "大家最先问的几个问题",
        faq: [
            Beneath {
                heading: "试用结束会怎样？",
                body: "工作区会停止对外服务，但不会删除任何东西。你的数据原样留着，直到你选定方案，或者请我们删除。",
            },
            Beneath {
                heading: "是按应用收费吗？",
                body: "不是。按登录使用的人数收费。打开库存不会改变账单。",
            },
            Beneath {
                heading: "可以自己部署吗？",
                body: "可以，在企业版方案里。它就是一个可执行文件加一个数据库，这是有意为之。",
            },
            Beneath {
                heading: "我的数据会和别人混在一起吗？",
                body: "不会。每个工作区都有自己的数据库和自己的地址。这是设计本身，不是付费升级项。",
            },
        ],
    },

    about: About {
        title: "关于",
        description: "我们为什么要做一套小公司真的负担得起的管理软件。",
        eyebrow: "关于",
        headline: "好软件不该只有大公司才用得上。",
        body: [
            "多数企业要么靠一张只有一个人看得懂的表格运转，要么靠一套配置费比购买费还贵的系统。两者其实是同一个问题换了件衣服：软件从来没有照着工作的形状来做，于是总得有个人在脑子里补上那段差距。",
            "我们要做的是另一种东西。库存在库位之间移动，因为库存本来就是这样动的；分录必须先平衡才能过账，因为分录本来就是这么回事。当软件照着工作建模，做这份工作的人一个下午就能学会 —— 让它“人人用得上”的，这一点远比价格更要紧。",
            "价格当然也重要。每个工作区都有自己的数据库、自己的地址和自己的权限，这些都不会被留作升级项来卖。小公司拿到的产品和大公司是同一个，因为做两个产品的结果，就是小的那个变成差的那个。",
        ],
        values_title: "我们坚持的几件事",
        cta_title: "自己看看吧。",
        cta_body: "要判断这些说法，最快的办法就是把它打开。",
    },

    contact: Contact {
        title: "联系",
        description: "找个人聊聊，或者直接开一个属于你的工作区。",
        eyebrow: "联系",
        headline: "有什么尽管问。",
        lede: "无论是想弄清楚某件事是怎么建模的，还是想知道它合不合你的做法 —— 一个实在的回答胜过一场演示。",
        cards: [
            Beneath {
                heading: "发封邮件",
                body: "找到能给出像样答复的人，这是最快的路。",
            },
            Beneath {
                heading: "开一个工作区",
                body: "大约一分钟，无需信用卡，之后就归你了。",
            },
            Beneath {
                heading: "登录",
                body: "已经有工作区了？它在自己的专属地址上。",
            },
        ],
    },

    solutions: Solutions {
        title: "解决方案",
        description: "同一个产品，贴合你所在行业真正的计数方式。",
        eyebrow: "解决方案",
        headline: "同一个产品，你自己的计数方式。",
        lede: "每家企业都要管钱、管物、管人。不同的是「物」到底指什么，以及它必须如何入账。下面是这个产品目前已经贴合的场景。",
        by_industry: "按行业",
        by_need: "按需要",
        menu_foot: "没列到你这一行？这些应用本身是通用的 —— 多数行业的差别只在于怎么配置。",
        industries: [
            Industry {
                name: "医疗健康",
                note: "诊所与药房，库存是有有效期的。",
                body: "药房的库存并不能互相替代：同一种药的两盒，如果其中一盒三月就到期，那它们就是两样东西。库存把批次与有效期作为移动本身的一部分来处理，而不是在旁边记一笔备注；某个批次过期时，总账也会看到这笔损失。",
                points: &[
                    "每一次移动都带批次与有效期",
                    "报废自动过账到总账",
                    "为调剂室、病区与隔离区分别设库位",
                ],
            },
            Industry {
                name: "零售与批发",
                note: "多个地点，一套数字。",
                body: "三家门店加一个后仓的库存，其实是同一个问题问了四遍。因为这里的移动永远发生在两个地点之间，所以「河畔店还有多少」和「我们一共有多少」是同一个查询换个筛选条件 —— 而不是两份到周五就对不上的报表。",
                points: &[
                    "每个点位一个仓库，之间可以调拨",
                    "计价方式由类别决定，不靠猜",
                    "有变体，尺码和颜色不必新建物料",
                ],
            },
            Industry {
                name: "生产制造",
                note: "投入了什么，产出了什么，成本是多少。",
                body: "生产本身也是一次移动：材料离开一个库位，成品到达另一个库位，两者的差额是一笔必须落进总账的成本。库存模型本就是照这个形状建的，而不是在旁边另加一块。",
                points: &[
                    "单位与单位类别，公斤和克是同一件事",
                    "估值与出库规则设在物料类别上",
                    "需要的成品可以用序列号",
                ],
            },
            Industry {
                name: "专业服务",
                note: "没有库存。部门就是成本中心。",
                body: "靠卖工时的公司，仓库里几乎没有东西，所有问题都变成「这笔成本落在业务的哪一块」。这里的成本中心是分录行上的一个维度，所以答案就在总账里，而不在照着总账另做的表格里。",
                points: &[
                    "部门，以及其中承担成本的那些",
                    "可以只用账簿不用库存 —— 应用之间是分开的",
                    "涉外业务可用多币种计价",
                ],
            },
            Industry {
                name: "教育",
                note: "各项经费不能混在一起。",
                body: "学校的钱进来时往往是带条件的，要紧的是能说清每一笔来自哪里。这需要一份可以自己塑形的科目表，以及每一行上的一个维度 —— 这两样都已经有了。",
                points: &[
                    "科目表由你自己塑形",
                    "会计期间要明确关闭，关了就是关了",
                    "审计轨迹在事情发生时就写下来",
                ],
            },
            Industry {
                name: "公益组织",
                note: "小团队，还有一份别人要审的报告。",
                body: "难的往往不是记账本身，而是三个人在别的工作之余顺手在记，并且每年总有一位不在这儿上班的人要能看懂它。这两件事都指向同一个结论：软件要把工作老老实实地建模出来。",
                points: &[
                    "团队还小的时候一直免费",
                    "有权限，志愿者只看得到自己那一块",
                    "每一次改动都记下是谁、在什么时候",
                ],
            },
        ],
        cta_title: "不确定合不合用？",
        cta_body: "开一个工作区，把一周的真实数字跑一遍。那比我们怎么说都快。",
    },

    not_found: NotFound {
        title: "页面不存在",
        heading: "这个地址上没有页面。",
        detail: "链接可能已经过期，也可能是输错了。",
    },

    apps: [
        AppCopy {
            name: "账簿",
            tagline: "复式记账，会计一看就认得的那种。",
            points: &[
                "可以自己塑形的科目表，带上总账需要的各种角色",
                "分录先平衡才过账，绝不事后补平",
                "会计年度与会计期间，开与关都要明确动作",
                "多币种须有在册汇率，否则拒绝入账",
            ],
            status: None,
        },
        AppCopy {
            name: "库存",
            tagline: "库存是场所之间的移动，不是某一列里的一个数字。",
            points: &[
                "物料、变体，以及计量它们的单位",
                "仓库、库位，以及它们之间的移动",
                "由类别决定计价方式、估值与出库规则",
                "批次与序列号，该管有效期的地方就管",
            ],
            status: Some("持续完善"),
        },
        AppCopy {
            name: "人员",
            tagline: "谁在这里工作，以及他们计入业务的哪一块成本。",
            points: &[
                "部门，以及其中作为成本中心的那些",
                "成本中心是分录行上的一个维度，不是报表上的一个筛选条件",
            ],
            status: Some("初期"),
        },
    ],

    pillars: [
        PillarCopy {
            heading: "它照着真实的东西建模",
            body: "库存在库位之间移动，因为库存本来就是这样动的；分录必须先平衡才能过账，因为分录本来就是这么回事。照着工作建模的软件，你才讲得清给正在做这份工作的人听。",
        },
        PillarCopy {
            heading: "你的工作区是你的",
            body: "每个工作区一个数据库，有自己的地址、自己的用户和自己的权限。不是共享表上加一个租户字段，再靠所有人记得带上筛选条件。",
        },
        PillarCopy {
            heading: "只加你需要的",
            body: "每个应用都是独立的一块。可以只要会计不要库存，或者只要库存不要薪酬，等下一块真正开始要紧的那天再打开它。",
        },
        PillarCopy {
            heading: "在你干活的地方够快",
            body: "紧凑的界面、以键盘为先的表格，以及为整天使用而不是为截图而定的字号。这里不会让你盯着转圈，只为看一个你早就知道的数字。",
        },
    ],

    plans: [
        PlanCopy {
            name: "入门版",
            who: "一个刚站稳脚跟的团队。",
            price: "免费",
            cadence: "在你起步的阶段",
            points: &["一个工作区", "最多三个人", "账簿与库存", "社区支持"],
            action: "免费开始",
        },
        PlanCopy {
            name: "商业版",
            who: "每天都靠它运转的公司。",
            price: "—",
            cadence: "每人每月",
            points: &[
                "包含入门版的全部",
                "人数不限",
                "全部应用",
                "多币种与税务",
                "邮件支持",
            ],
            action: "免费开始",
        },
        PlanCopy {
            name: "企业版",
            who: "多个法人实体，还有自己的规矩。",
            price: "面谈",
            cadence: "",
            points: &[
                "包含商业版的全部",
                "多个工作区",
                "部署在你自己的环境",
                "上线导入与数据迁移",
            ],
            action: "联系我们",
        },
    ],
};
