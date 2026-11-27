import { createApi, fetchBaseQuery } from '@reduxjs/toolkit/query/react';
import type { BaseQueryFn, FetchArgs, FetchBaseQueryError } from '@reduxjs/toolkit/query';
import { getApiBaseUrl, getHeaders } from '../services/api';
import type {
  DesktopConfig,
  InstallJobSnapshot,
  PythonPackagesStatus,
  SetupStatusResponse,
} from '../types';

const dynamicBaseQuery: BaseQueryFn<string | FetchArgs, unknown, FetchBaseQueryError> = async (
  args,
  api,
  extraOptions
) => {
  const rawQuery = fetchBaseQuery({
    baseUrl: getApiBaseUrl(),
    prepareHeaders: (headers) => {
      const auth = getHeaders();
      Object.entries(auth).forEach(([k, v]) => headers.set(k, v));
      return headers;
    },
  });
  return rawQuery(args, api, extraOptions);
};

export const setupApi = createApi({
  reducerPath: 'setupApi',
  baseQuery: dynamicBaseQuery,
  tagTypes: ['SetupStatus', 'InstallProgress'],
  endpoints: (builder) => ({
    getSetupStatus: builder.query<SetupStatusResponse, void>({
      query: () => '/v1/system/setup-status',
      providesTags: ['SetupStatus'],
    }),
    getInstallProgress: builder.query<InstallJobSnapshot, void>({
      query: () => '/v1/system/setup/progress',
      providesTags: ['InstallProgress'],
    }),
    saveSetupConfig: builder.mutation<
      DesktopConfig,
      {
        setup_completed?: boolean;
        runtime_profile?: string;
        runtime_dir?: string;
        custom_python?: string | null;
        remote_server_url?: string | null;
        telemetry_opt_in?: boolean;
      }
    >({
      query: (body) => ({
        url: '/v1/system/setup/config',
        method: 'POST',
        body,
      }),
      invalidatesTags: ['SetupStatus'],
    }),
    probePython: builder.mutation<
      {
        valid: boolean;
        path: string;
        version?: string;
        packages?: PythonPackagesStatus;
        error?: string;
      },
      { path: string }
    >({
      query: (body) => ({
        url: '/v1/system/setup/probe-python',
        method: 'POST',
        body,
      }),
    }),
    testRemoteServer: builder.mutation<
      { reachable: boolean; url: string; status?: string; version?: string; error?: string },
      { url: string }
    >({
      query: (body) => ({
        url: '/v1/system/setup/test-remote',
        method: 'POST',
        body,
      }),
    }),
    startSetupInstall: builder.mutation<
      InstallJobSnapshot,
      {
        profile: string;
        runtime_dir: string;
        custom_python?: string;
        telemetry_opt_in: boolean;
      }
    >({
      query: (body) => ({
        url: '/v1/system/setup/install',
        method: 'POST',
        body,
      }),
      invalidatesTags: ['InstallProgress'],
    }),
    cancelSetupInstall: builder.mutation<InstallJobSnapshot, void>({
      query: () => ({
        url: '/v1/system/setup/cancel',
        method: 'POST',
        body: {},
      }),
      invalidatesTags: ['InstallProgress'],
    }),
    installNativePiper: builder.mutation<
      {
        status: string;
        target: string;
        python_required: boolean;
        binary_path: string;
        message: string;
      },
      void
    >({
      query: () => ({
        url: '/v1/system/runtimes/install-native',
        method: 'POST',
        body: { target: 'piper' },
      }),
      invalidatesTags: ['SetupStatus'],
    }),
  }),
});

export const {
  useGetSetupStatusQuery,
  useGetInstallProgressQuery,
  useSaveSetupConfigMutation,
  useProbePythonMutation,
  useTestRemoteServerMutation,
  useStartSetupInstallMutation,
  useCancelSetupInstallMutation,
  useInstallNativePiperMutation,
} = setupApi;
