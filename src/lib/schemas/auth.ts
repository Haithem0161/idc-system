import { z } from "zod"

export const UserRoleSchema = z.enum(["superadmin", "receptionist", "accountant"])
export type UserRole = z.infer<typeof UserRoleSchema>

export const LoginSchema = z.object({
  email: z.email("invalid email"),
  password: z.string().min(8, "password must be at least 8 characters"),
})
export type LoginInput = z.infer<typeof LoginSchema>

export const UnlockSchema = z.object({
  password: z.string().min(8),
})

export const FirstAdminSchema = z.object({
  email: z.email(),
  name: z.string().min(1, "name is required"),
  password: z.string().min(8),
  entity_id: z.string().optional(),
})

export const UserCreateSchema = z.object({
  email: z.email(),
  name: z.string().min(1),
  role: UserRoleSchema,
  password: z.string().min(8),
})

export const UserUpdateSchema = z.object({
  email: z.email().optional(),
  name: z.string().min(1).optional(),
  role: UserRoleSchema.optional(),
})

export const ResetPasswordSchema = z.object({
  new_password: z.string().min(8),
})
