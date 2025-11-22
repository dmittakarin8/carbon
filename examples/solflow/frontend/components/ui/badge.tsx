import * as React from "react"

export interface BadgeProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: 'default' | 'success' | 'info' | 'warning' | 'danger' | 'neutral'
}

const Badge = React.forwardRef<HTMLDivElement, BadgeProps>(
  ({ className = '', variant = 'default', ...props }, ref) => {
    const variantClasses = {
      default: 'bg-gray-700 text-gray-200 border-gray-600',
      success: 'bg-green-900/40 text-green-300 border-green-700',
      info: 'bg-blue-900/40 text-blue-300 border-blue-700',
      warning: 'bg-orange-900/40 text-orange-300 border-orange-700',
      danger: 'bg-red-900/40 text-red-300 border-red-700',
      neutral: 'bg-gray-800/60 text-gray-400 border-gray-700',
    }

    return (
      <div
        ref={ref}
        className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-medium border ${variantClasses[variant]} ${className}`}
        {...props}
      />
    )
  }
)

Badge.displayName = "Badge"

export { Badge }
