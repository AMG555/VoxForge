import { configureStore } from '@reduxjs/toolkit';
import { TypedUseSelectorHook, useDispatch, useSelector } from 'react-redux';
import setupReducer from './setupSlice';
import { setupApi } from './setupApi';

export const store = configureStore({
  reducer: {
    setup: setupReducer,
    [setupApi.reducerPath]: setupApi.reducer,
  },
  middleware: (getDefaultMiddleware) => getDefaultMiddleware().concat(setupApi.middleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;

export const useAppDispatch: () => AppDispatch = useDispatch;
export const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;
