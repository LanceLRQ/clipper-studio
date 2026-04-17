import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // TanStack Router 文件路由约定要求 `export const Route = createFileRoute(...)`
      // 与同文件的页面组件一起导出，与该规则天然冲突
      'react-refresh/only-export-components': 'off',
      // React 19 新增规则对"加载态 → fetch → setState"模式过于严格，这里按警告处理
      'react-hooks/set-state-in-effect': 'warn',
      // 以 `_` 前缀命名的变量/参数视为刻意保留
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
    },
  },
])
