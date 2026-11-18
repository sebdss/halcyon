import {Config} from '@remotion/cli/config';

// Rendu reproductible : JPEG intermédiaire de haute qualité, H.264 final.
Config.setVideoImageFormat('jpeg');
Config.setJpegQuality(95);
Config.setCodec('h264');
Config.setCrf(16);
Config.setPixelFormat('yuv420p');
Config.setConcurrency(2);
// Chrome installé sur la machine (sinon Remotion télécharge le sien).
if (process.env.REMOTION_CHROME) {
  Config.setBrowserExecutable(process.env.REMOTION_CHROME);
}
