package store;
public class Entry {
        static Class<?> identity(Class<?> value) { return value; }
        static Object echo(Object value) { return value; }
        public static void main(String[] args) {
            State box = new State();
            Class<?>[] types = new Class<?>[3];
            State.array = types;
            types[0] = identity(Later.class);
            State.global = echo(types[0]);
            box.field = State.global;
            if (box.field == Later.class && box.field instanceof Class) State.flags |= 1;
            State.array[1] = int[][].class;
            if (types[1] == new int[0][].getClass()) State.flags |= 2;
            State.array[2] = Later[].class;
            if (State.array[2] == new Later[1].getClass()) State.flags |= 4;
            if (types.getClass() == Class[].class && box.field.getClass() == Class.class) State.flags |= 8;
            synchronized(box.field) {
                synchronized(Later.class) {
                    if (identity((Class<?>)box.field) == Later.class) State.flags |= 16;
                }
            }
            if (State.initialized == 0) State.flags |= 32;
        }
    }
