import {
  cloneElement,
  forwardRef,
  isValidElement,
  useId,
  type InputHTMLAttributes,
  type LabelHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import { cn } from "@/lib/utils";

const field =
  "h-10 w-full rounded-md border bg-background px-3 text-sm text-foreground placeholder:text-muted-foreground " +
  "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:opacity-50";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) => (
  <input ref={ref} className={cn(field, className)} {...p} />
));
Input.displayName = "Input";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className, ...p }, ref) => <select ref={ref} className={cn(field, "cursor-pointer pr-8", className)} {...p} />,
);
Select.displayName = "Select";

export function Label({ className, ...p }: LabelHTMLAttributes<HTMLLabelElement>) {
  return <label className={cn("mb-1.5 block text-sm font-medium", className)} {...p} />;
}

/**
 * A label, a control and optional help text. A single <Input> or <Select> is
 * tied to the label (so clicking the label focuses it and screen readers read
 * it); anything else is labelled as a group.
 */
export function Field({ label, help, children }: { label: string; help?: string; children: ReactNode }) {
  const id = useId();
  const helpId = help ? `${id}-help` : undefined;
  if (isValidElement<{ id?: string }>(children) && (children.type === Input || children.type === Select)) {
    const controlId = children.props.id ?? id;
    return (
      <div>
        <Label htmlFor={controlId}>{label}</Label>
        {cloneElement(children as React.ReactElement<Record<string, unknown>>, {
          id: controlId,
          "aria-describedby": helpId,
        })}
        {help && (
          <p id={helpId} className="mt-1 text-xs text-muted-foreground">
            {help}
          </p>
        )}
      </div>
    );
  }
  return (
    <div role="group" aria-labelledby={`${id}-label`} aria-describedby={helpId}>
      <p id={`${id}-label`} className="mb-1.5 block text-sm font-medium">
        {label}
      </p>
      {children}
      {help && (
        <p id={helpId} className="mt-1 text-xs text-muted-foreground">
          {help}
        </p>
      )}
    </div>
  );
}
