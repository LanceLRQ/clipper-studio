import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'ClipperStudio',
  description: '面向直播录播切片创作者的桌面视频工作台',
  base: '/clipper-studio/',

  head: [['link', { rel: 'icon', href: '/clipper-studio/favicon.ico' }]],

  themeConfig: {
    nav: [
      { text: '使用指南', link: '/getting-started' },
      { text: '常见问题', link: '/faq' },
    ],

    sidebar: [
      {
        text: '开始使用',
        items: [
          { text: '简介', link: '/getting-started' },
          { text: '安装与启动', link: '/installation' },
        ],
      },
      {
        text: '核心功能',
        items: [
          { text: '工作区管理', link: '/workspace' },
          { text: '视频工作台', link: '/video-workbench' },
          { text: '字幕系统', link: '/subtitle' },
          { text: '弹幕系统', link: '/danmaku' },
        ],
      },
      {
        text: '进阶',
        items: [
          { text: '插件系统', link: '/plugins' },
          { text: '设置与依赖管理', link: '/settings' },
        ],
      },
      {
        text: '其他',
        items: [
          { text: '常见问题', link: '/faq' },
        ],
      },
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/LanceLRQ/clipper-studio' },
    ],

    search: {
      provider: 'local',
    },

    footer: {
      message: '基于 GPLv3 协议发布',
    },
  },
})
