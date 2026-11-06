import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap font-medium transition-colors duration-120 disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-3.5 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-accent-fill text-accent-foreground hover:opacity-90 active:opacity-80",
        secondary: "border border-border-strong bg-panel text-foreground hover:bg-hover",
        ghost: "text-foreground hover:bg-hover",
        destructive: "bg-critical text-white hover:opacity-90 active:opacity-80",
        subtle: "text-accent hover:bg-hover",
      },
      size: {
        sm: "h-6 rounded-md px-2 text-[12px]",
        default: "h-7 rounded-md px-3 text-[13px]",
        lg: "h-8 rounded-md px-4 text-[13px]",
        icon: "h-7 w-7 rounded-md",
      },
    },
    defaultVariants: { variant: "secondary", size: "default" },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, type = "button", ...props }, ref) => (
    <button ref={ref} type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />
  ),
);
Button.displayName = "Button";
export { buttonVariants };
