import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "ui-button rounded-control outline-none transition-colors focus-visible:ring-[3px] focus-visible:ring-ring/20 disabled:pointer-events-none disabled:opacity-60",
  {
    variants: {
      variant: {
        primary: "primary-button bg-primary text-primary-foreground",
        secondary: "secondary-button border-border bg-card text-card-foreground",
        ghost: "ui-ghost-button text-muted-foreground hover:bg-muted hover:text-foreground",
        dangerGhost: "ui-danger-ghost-button text-muted-foreground hover:text-destructive"
      }
    },
    defaultVariants: {
      variant: "secondary"
    }
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return <Comp className={cn(buttonVariants({ variant }), className)} ref={ref} {...props} />;
  }
);
Button.displayName = "Button";

export { Button };
