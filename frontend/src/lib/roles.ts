import type { User } from './api'

export function isRoot(user: User | null | undefined) {
  return user?.role === 'root'
}

export function isOperator(user: User | null | undefined) {
  return !!user && ['requester', 'user', 'approver'].includes(user.role)
}

export function isRequester(user: User | null | undefined) {
  return user?.role === 'requester'
}

export function canApprove(user: User | null | undefined) {
  return !!user && (user.role === 'approver' || user.role === 'root')
}

export function canStartDownloads(user: User | null | undefined) {
  return !!user && (user.role === 'user' || user.role === 'approver')
}
