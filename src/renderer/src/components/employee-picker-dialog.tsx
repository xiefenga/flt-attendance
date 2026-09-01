import { useEffect, useMemo, useRef, useState } from "react";
import {
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  useReactTable,
  type ColumnDef,
  type RowSelectionState,
  type Table
} from "@tanstack/react-table";
import { Search, Users } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

import type { EmployeeIdentity } from "../../../shared/ipc-contract";

interface EmployeePickerDialogProps {
  employees: EmployeeIdentity[];
  open: boolean;
  targetLabel: string;
  onOpenChange(open: boolean): void;
  onConfirm(employees: EmployeeIdentity[]): void;
}

interface TableCheckboxProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type"> {
  indeterminate?: boolean;
}

function TableCheckbox({ indeterminate = false, ...props }: TableCheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (ref.current) ref.current.indeterminate = indeterminate;
  }, [indeterminate]);

  return <input ref={ref} className="employee-table-checkbox" type="checkbox" {...props} />;
}

function SelectFilteredRowsCheckbox({ table }: { table: Table<EmployeeIdentity> }) {
  const filteredRows = table.getFilteredRowModel().rows;
  const selectedRows = filteredRows.filter((row) => row.getIsSelected());

  return (
    <TableCheckbox
      aria-label="选择当前筛选结果"
      checked={filteredRows.length > 0 && selectedRows.length === filteredRows.length}
      disabled={filteredRows.length === 0}
      indeterminate={selectedRows.length > 0 && selectedRows.length < filteredRows.length}
      onChange={(event) => {
        for (const row of filteredRows) row.toggleSelected(event.target.checked);
      }}
    />
  );
}

export function EmployeePickerDialog({
  employees,
  open,
  targetLabel,
  onOpenChange,
  onConfirm
}: EmployeePickerDialogProps) {
  const [filter, setFilter] = useState("");
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});

  useEffect(() => {
    if (open) {
      setFilter("");
      setRowSelection({});
    }
  }, [open]);

  const columns = useMemo<ColumnDef<EmployeeIdentity>[]>(
    () => [
      {
        id: "select",
        header: ({ table }) => <SelectFilteredRowsCheckbox table={table} />,
        cell: ({ row }) => (
          <TableCheckbox
            aria-label={`选择${row.original.name}`}
            checked={row.getIsSelected()}
            onChange={row.getToggleSelectedHandler()}
          />
        ),
        enableGlobalFilter: false
      },
      {
        accessorKey: "name",
        header: "姓名",
        cell: ({ getValue }) => <strong>{getValue<string>()}</strong>
      },
      {
        accessorKey: "employeeNo",
        header: "工号",
        cell: ({ getValue }) => getValue<string>() || <span className="employee-number-empty">无工号</span>
      }
    ],
    []
  );

  const table = useReactTable({
    data: employees,
    columns,
    state: {
      globalFilter: filter,
      rowSelection
    },
    getRowId: (employee, index) => `${employee.employeeNo}:${employee.name}:${index}`,
    enableRowSelection: true,
    onGlobalFilterChange: setFilter,
    onRowSelectionChange: setRowSelection,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    globalFilterFn: (row, _columnId, value) => {
      const query = String(value).trim().toLocaleLowerCase();
      if (!query) return true;
      return `${row.original.name} ${row.original.employeeNo}`
        .toLocaleLowerCase()
        .includes(query);
    }
  });

  const selectedEmployees = table.getSelectedRowModel().rows.map((row) => row.original);
  const filteredCount = table.getFilteredRowModel().rows.length;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="employee-picker-dialog"
        overlayClassName="employee-picker-backdrop"
      >
        <div className="employee-picker-panel">
          <header className="employee-picker-heading">
            <div>
              <DialogTitle>选择人员</DialogTitle>
              <DialogDescription>
                选择后将添加到“{targetLabel}”，支持一次添加多人。
              </DialogDescription>
            </div>
          </header>

          <div className="employee-picker-filter">
            <Search size={16} aria-hidden="true" />
            <Input
              autoFocus
              aria-label="筛选人员"
              placeholder="按姓名或工号筛选"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
            />
            <span>{filter ? `${filteredCount} 条结果` : `共 ${employees.length} 人`}</span>
          </div>

          <div className="employee-table-frame">
            <div className="employee-table-scroll">
              <table className="employee-table">
                <thead>
                  {table.getHeaderGroups().map((headerGroup) => (
                    <tr key={headerGroup.id}>
                      {headerGroup.headers.map((header) => (
                        <th key={header.id}>
                          {header.isPlaceholder
                            ? null
                            : flexRender(header.column.columnDef.header, header.getContext())}
                        </th>
                      ))}
                    </tr>
                  ))}
                </thead>
                <tbody>
                  {table.getRowModel().rows.map((row) => (
                    <tr
                      data-selected={row.getIsSelected() ? "true" : undefined}
                      key={row.id}
                      onClick={() => row.toggleSelected()}
                    >
                      {row.getVisibleCells().map((cell) => (
                        <td key={cell.id} onClick={cell.column.id === "select" ? (event) => event.stopPropagation() : undefined}>
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
              {filteredCount === 0 ? (
                <div className="employee-table-empty">
                  <Users size={22} aria-hidden="true" />
                  <strong>{employees.length ? "没有匹配的人员" : "没有可添加的人员"}</strong>
                  <span>{employees.length ? "试试其他姓名或工号" : "当前报表人员均已配置"}</span>
                </div>
              ) : null}
            </div>
          </div>

          <footer className="employee-picker-footer">
            <span>已选择 {selectedEmployees.length} 人</span>
            <div>
              <Button type="button" onClick={() => onOpenChange(false)}>取消</Button>
              <Button
                variant="primary"
                type="button"
                disabled={selectedEmployees.length === 0}
                onClick={() => {
                  onConfirm(selectedEmployees);
                  onOpenChange(false);
                }}
              >
                添加{selectedEmployees.length ? ` ${selectedEmployees.length} 人` : ""}
              </Button>
            </div>
          </footer>
        </div>
      </DialogContent>
    </Dialog>
  );
}
